# Review/Cleanup Pass — 2026-04-08

## Scope

Reviewed the 10 most recent commits (11d03e3..cb2f0e5) on the Yang boolean
pipeline. Audited for governance violations (P9, A14.3), tolerance hardcoding,
workarounds, and weak tests.

## Commit Review Summary

| Commit | Verdict | Notes |
|--------|---------|-------|
| cb2f0e5 | **Good** | Replaced hardcoded 1e-20 with TAU_NORMALIZE_SQ in exact_mesh.rs tests |
| ff67920 | **Good** | Strengthened g2_union_face_count oracle documentation |
| c1e0555 | **Good with caveats** | Fixed Euler validator for compound solids; empty result path needed cleanup (see Fixes below) |
| b77f66f | **Good** | Fixed boundary detection for non-surviving adjacency; proper algorithm with tests |
| 329e6ab | **Good** | Documentation sharpening only |
| ce0a578 | **Acceptable** | Added #[ignore] to 5 red-phase P3 tests — correct P3 workflow |
| a4b1f27 | **Good** | Audit review summary |
| 980a4ff | **Good** | Replaced hardcoded 1e-20 and 1e-12 with unit constants in test helper |
| 2d0d4a6 | **Good** | Red-phase tests for curved-surface retessellation (P3) |
| 11d03e3 | **Good** | Red-phase box+cylinder pipeline tests (P3) |

## Tolerance Audit (A14.3)

### Production code: CLEAN
No hardcoded tolerance constants found in non-test, non-units.rs production
code in the recently changed files (yang_integration.rs, topology_extract.rs,
exact_mesh.rs). Previous audit passes (980a4ff, cb2f0e5) successfully cleaned
these up.

### Test code: SYSTEMIC ISSUE (not addressed this pass)
`waffle_kernel_tests.rs` has ~100+ instances of hardcoded `1e-6`, `1e-9`,
`1e-12` etc. in assertions. These predate the recent commits and are a
pervasive codebase-wide issue. Fixing all of them is a separate effort
that should be tracked in PLAN.md. The recent commits did NOT introduce
new hardcoded tolerances.

## Workaround Detection (P9)

### "mesh passthrough" comments were misleading (FIXED)
- `waffle_kernel.rs:2425-2428` described the cached render mesh return as
  "mesh passthrough bypassing retessellation" — implying sub-triangle mesh
  was being used directly. In reality, `yang_boolean_inner` Step 9 properly
  retessellates the result B-Rep at Render LOD and caches the result.
  The tessellate() method simply returns this pre-computed mesh.
- Same misleading language in WaffleSolid doc comment (line 46-50) and
  BooleanResult doc comment (mod.rs:83).
- **Fixed**: Updated all three comments to accurately describe the caching
  mechanism.

### Existing fallback patterns: ACCEPTABLE (A15.6 deprecated code)
Many "fallback" hits in waffle_kernel.rs and boolean/analytical.rs refer to
the deprecated S-H clipping + polygon approximation pipeline. Per A15.6,
this code is deprecated but not yet removable (Yang pipeline must be
operational first). No new fallback paths were introduced by recent commits.

### `build_render_mesh_from_survival` is properly gated
The sub-triangle mesh builder is `#[cfg(test)]` only (yang_integration.rs:257).
Production code uses proper retessellation. No P9 violation.

## Fixes Applied

### 1. Corrected misleading "mesh passthrough" comments
- `waffle_kernel.rs:2425-2428`: Changed from "mesh passthrough bypassing
  retessellation" to "Return pre-computed render mesh from Yang pipeline
  retessellation."
- `waffle_kernel.rs:46-50` (WaffleSolid doc): Updated to describe caching
  from retessellation, not mesh boolean output.
- `boolean/mod.rs:83` (BooleanResult doc): Same fix.

### 2. Simplified empty result path in yang_integration.rs
The empty result path (c1e0555) built a full `WaffleSolid` with 15 fields
then immediately converted it to `BooleanResult` via
`waffle_solid_to_boolean_result()`. Simplified to construct `BooleanResult`
directly, eliminating the unnecessary WaffleSolid intermediary and avoiding
coupling to WaffleSolid field additions.

## Findings NOT Requiring Fixes

- **g2_union_face_count `>= 6`**: The weakening from `>= 10` to `>= 6` is
  geometrically sound. With 1D-offset boxes, Yang pipeline correctly merges
  coplanar faces to produce exactly 6. Legacy produces 14. The `>= 6` bound
  accepts both. Companion oracles (volume g1, Euler g3) independently validate
  correctness. The comment added in ff67920 documents this well.

- **Red-phase #[ignore] tests**: Five tests were given #[ignore] in ce0a578.
  This is correct P3 workflow — they document known bugs and will be un-ignored
  when the fixes land. No P9 violation.

## Recommendations for Future Passes

1. **Centralize test tolerances**: Create a `TAU_TEST_ASSERT` constant in
   units.rs for the common `1e-6` assertion tolerance used in ~100+ test
   assertions in waffle_kernel_tests.rs. This is a large but mechanical change.

2. **Remove stale "passthrough" test naming**: Tests like
   `yang_mesh_passthrough_*` still use "passthrough" in their names even
   though production code uses retessellation. Consider renaming to
   `yang_mesh_cached_*` or `yang_mesh_subtri_*` to reflect their actual
   purpose (testing the #[cfg(test)] sub-triangle builder).
