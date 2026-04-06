# Review/Cleanup Pass #2 — 2026-04-06

## Scope

Adversarial review of the 5 most recent commits on the kernel codebase, checking
for governance violations (P9/hack-to-green, A14.3/tolerance centralization, P1/test
oracles, A15.6/no S-H fallback) and workaround patterns.

## Commits Reviewed

| Hash | Message | Verdict |
|------|---------|---------|
| 2d0d4a6 | test(kernel): add red-phase tests for Yang curved-surface retessellation (P3) | **MINOR — hardcoded tolerances in test helper** |
| 11d03e3 | test(kernel): add failing box+cylinder Yang pipeline tests (P3 red phase) | CLEAN |
| 84dbea2 | fix(kernel): use centroid-fan tessellation for convex polygons with collinear vertices | CLEAN |
| ee644b6 | audit(kernel): add review summary for 2026-04-06 cleanup pass | CLEAN |
| 92174f0 | audit(kernel): revert P9-violating fallback paths in Yang render mesh pipeline | CLEAN |

## Findings

### Step 1: Commit Review

**2d0d4a6** — Three red-phase tests for curved-surface retessellation. Good P3 compliance
(tests fail before implementation). Tests have real numeric oracles (self-intersection
count == 0, exact vertex position matching, angular range checking). One issue:
the `count_mesh_self_intersections` test helper used hardcoded `1e-20` (degenerate
axis guard) and `1e-12` (SAT separation tolerance) instead of `TAU_NORMALIZE_SQ`
and `TAU_WORK`. Fixed.

**11d03e3** — Two red-phase tests for box+cylinder Yang pipeline. Proper P3 compliance.
Tests use Euler characteristic and face count oracles. Detailed root-cause analysis
in commit message. Clean.

**84dbea2** — Centroid-fan tessellation fix for convex polygons with collinear vertices.
Uses `TAU_NORMALIZE` constant correctly. Includes a proper test with degenerate-triangle
assertion. Un-ignores two previously-blocked tests (B1, B2). Clean.

**ee644b6** — Documentation only (review summary). Clean.

**92174f0** — Reverts P9-violating fallback paths from earlier commits. Removes
`quick_mesh_has_unpaired_edges`, restores strict degenerate triangle test, removes
weak fallback test. All correct governance enforcement. Clean.

### Step 2: Tolerance Audit (A14.3)

**Production code**: No violations found. All `1e-N` literals in production (non-test)
kernel code use centralized constants from `units.rs`:
- `exact_mesh.rs:1367,1413,1534` — `TAU_WORK.sqrt()` with explanatory comment (correct)
- No raw epsilon literals in boolean, tessellation, or SSI production code

**Test code**: One violation in `count_mesh_self_intersections` (yang_integration.rs:2756,2761,2772) —
hardcoded `1e-20` and `1e-12` instead of `TAU_NORMALIZE_SQ` and `TAU_WORK`. **Fixed.**

### Step 3: Workaround Detection

Searched for: fallback, workaround, bypass, hack, accept.*invalid, skip.*validation,
cached mesh, passthrough, FIXME/TODO workaround.

**No new P9 violations** in the 5 reviewed commits. All "fallback" references are either:
- In deprecated S-H pipeline code (already tracked under A15.6 migration)
- In test code referencing the deprecated paths (with `#[ignore]` annotations)
- Legitimate mathematical fallbacks (e.g., cross-product normal when surface geometry unavailable)
- Comments documenting removed fallback paths

## Changes Made

### Commit: `audit(kernel): replace hardcoded 1e-20 and 1e-12 with unit constants in test helper`

- `yang_integration.rs:2756` — `1e-20` → `crate::units::TAU_NORMALIZE_SQ` (degenerate axis guard in SAT)
- `yang_integration.rs:2761` — `1e-12` → `crate::units::TAU_WORK` (SAT separation tolerance)
- `yang_integration.rs:2772` — `1e-12` → `crate::units::TAU_WORK` (AABB overlap tolerance)

All changes in test code only. Zero clippy warnings, clean formatting.

## Overall Assessment

The recent dev work is **mostly clean**. The prior cleanup pass (92174f0) correctly
identified and reverted P9 violations. The new red-phase tests (2d0d4a6, 11d03e3)
follow P3 properly with real numeric oracles. The centroid-fan fix (84dbea2) is a
well-scoped bug fix with an appropriate test.

The only issue found was minor: hardcoded tolerance literals in a test helper function.
No production code violations detected.

## Recommendations

1. The three red-phase tests from 2d0d4a6 document bugs in curved-surface handling
   (full 360° cylinder tessellation, twin-pairing failure on curved geometry). These
   should be the next implementation targets for the Yang pipeline.

2. Consider adding `TAU_NORMALIZE_SQ` to the grep pattern used in future audit passes
   to catch test code that uses the raw value instead of the constant.
