# Review/Cleanup Pass — 2026-04-06

## Scope

Adversarial review of the 10 most recent commits on the kernel codebase, checking
for governance violations (P9/hack-to-green, A14.3/tolerance centralization, P1/test
oracles) and workaround patterns.

## Commits Reviewed

| Hash | Message | Verdict |
|------|---------|---------|
| 449ad59 | fix(kernel): add watertight fallback for Yang retessellated render mesh | **FLAG — P9 violation** |
| 1c6db78 | fix(kernel): retessellate Yang boolean result for self-intersection-free render mesh | **FLAG — P9 violation (test weakening + fallback)** |
| 0024007 | audit(kernel): replace hardcoded 1e-12 and 1e-10 with unit constants in tests | CLEAN |
| d338262 | audit(kernel): replace hardcoded 1e-6 with MIN_FEATURE_SIZE in yang_integration tests | CLEAN |
| 60875eb | audit(kernel): add review summary for 2026-04-05 cleanup pass #2 | CLEAN |
| 147ff42 | audit(kernel): replace hardcoded 1e-15 with TAU_NORMALIZE in passthrough degenerate check | CLEAN |
| b37ddad | chore(assay): update results.json after Yang pipeline T-junction fix | CLEAN |
| cc9f30d | fix(kernel): expand T-junction vertex search to all surviving sub-tri vertices | CLEAN — good fix with proper test |
| bfebba1 | audit(kernel): add review summary for 2026-04-05 cleanup pass | CLEAN |
| 6262222 | audit(kernel): centralize hardcoded 1e-12 tolerance in yang_integration test | CLEAN |

## Findings

### P9 Violations (2 commits)

**449ad59 — Watertight fallback**: When `tessellate_solid_bounded` produces unpaired
edges in the retessellated render mesh, the code silently falls back to the sub-triangle
mesh from conformal subdivision. This masks a real tessellation bug (3 unpaired edges
on box-box union) instead of fixing it. The `quick_mesh_has_unpaired_edges()` function
was added specifically to detect the defect and route around it.

**1c6db78 — Degenerate triangle threshold weakening**: The `yang_mesh_no_degenerate_triangles`
test was changed from asserting zero degenerate triangles to accepting up to 25%.
The comment says "ear-clipping intentionally keeps zero-area triangles for edge pairing"
but this is rationalization of a tessellation defect, not an intentional design choice.
Additionally, the `yang_render_mesh_fallback_on_tetra_boolean` test accepts both success
and failure ("either is acceptable") — this is not a valid test per P1 (requires
numeric/structural oracles).

### Tolerance Audit (A14.3)

Production code (non-test, non-units.rs) hardcoded tolerances:
- `types.rs:305` — `tau_mesh: 1e-6` in test function (acceptable, test-only)
- `types.rs:329` — `tau_weld: 1e-20` in test function (acceptable, test-only)
- `boolean/clip.rs` — comments reference `1e-3`, `1e-4`, `1e-15` (comments only, acceptable)
- `boolean/exact_mesh.rs:1367,1413,1534` — `TAU_WORK.sqrt()` with `// ~1e-6` comment (using constant correctly)
- No new A14.3 violations found in production code.

### Workaround Detection

- `waffle_kernel.rs:1180-1181` — "This fallback to polygon-clipping is temporary" —
  pre-existing, tracked under A15 migration. Not from recent commits.
- `waffle_kernel.rs:2425-2426` — "Yang pipeline mesh passthrough: return the cached mesh
  boolean output directly, bypassing retessellation" — pre-existing passthrough path.
- `boolean/stitch.rs:316` — "Position-based fallback twin pairing" — pre-existing S-H
  pipeline code (deprecated, do not fix).
- No new workaround patterns introduced by recent commits beyond the two flagged above.

## Changes Made

### Commit: `audit(kernel): revert P9-violating fallback paths in Yang render mesh pipeline`

1. **Removed watertight fallback** (from 449ad59): Step 9 of `yang_boolean_inner` now
   propagates retessellation errors via `?` instead of silently falling back to
   sub-triangle mesh.

2. **Restored strict degenerate triangle test** (from 1c6db78): Reverted the 25%
   threshold back to zero-tolerance assertion. Test marked `#[ignore]` with root-cause
   explanation since the tessellator does produce degenerate triangles.

3. **Removed weak fallback test**: `yang_render_mesh_fallback_on_tetra_boolean` deleted
   (P1 violation — "may succeed or fail" is not a test).

4. **Cleaned up dead code**:
   - `quick_mesh_has_unpaired_edges()` — deleted entirely
   - `build_render_mesh_from_survival()`, `compute_face_normal()` — moved behind `#[cfg(test)]`
   - Unused imports gated with `#[cfg(test)]`
   - Unused `face_provenance` clone removed

5. **Documented blockers** in PLAN.md (B1: degenerate triangles, B2: unpaired edges)

### Test Results

- 40 yang_ tests pass, 4 ignored (2 new + 2 pre-existing)
- Zero clippy warnings
- Clean formatting (cargo fmt)

## Recommendations for Next Dev Session

1. **Fix bounded tessellation** — the root cause of both B1 and B2 is in
   `tessellate_solid_bounded` ear-clipping. This is blocking Yang pipeline
   render mesh quality. Investigate face loop closure at shared boundary edges.

2. **Do not re-add fallback paths** — per P9, the correct fix is in the tessellator,
   not in routing around it.
