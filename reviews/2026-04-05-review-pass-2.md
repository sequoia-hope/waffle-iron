# Review/Cleanup Pass #2 — 2026-04-05

**Branch:** `auto-waffle/2026-04-05T22-34-20`
**Reviewer:** auto-waffle (manager direct execution)
**Commits audited:** 9c524ab..b37ddad (10 commits)

---

## Methodology

1. Read governance documents (Constitution P1/P9/P10, Invariants A14.3/A15.6, DoD)
2. Reviewed diffs for all 10 recent commits via `git show`
3. Searched `crates/kernel/src/` for hardcoded tolerances outside `units.rs` (A14.3)
   — Python script filtered production code (excluding tests, units.rs, mock_kernel)
4. Searched for workaround/hack/fallback/bypass patterns (P9)
5. Checked test quality for numeric oracles (P1)

---

## Commit Review

| Hash | Message | Verdict |
|------|---------|---------|
| b37ddad | chore(assay): update results.json after T-junction fix | CLEAN — data-only, no code changes |
| cc9f30d | fix(kernel): expand T-junction vertex search | CLEAN — proper fix with test, cites [#24][#9] |
| bfebba1 | audit(kernel): add review summary | CLEAN — documentation only |
| 6262222 | audit(kernel): centralize 1e-12 in yang_integration test | CLEAN — A14.3 fix |
| fb2d783 | fix(kernel): (position, normal) vertex dedup | CLEAN — fixes real bug, good test with 95% oracle |
| 61a9d7d | audit(specs): update yang_error_fallback.md | CLEAN — spec alignment |
| cc3e733 | audit(kernel): fix stale fallback comments | CLEAN — tightened oracles, centralized TAU_WORK |
| f7f91a1 | audit(kernel): fix stale comment | CLEAN — comment-only |
| 5587e00 | fix(kernel): thread deadline through label_cells | CLEAN — proper timeout, 3 new tests |
| 9c524ab | audit(kernel): review/cleanup pass | CLEAN — documentation only |

### Quality Assessment

- **Commit messages:** All clearly explain WHY, not just what. Good.
- **Test oracles:** All new tests have numeric oracles (normal agreement %, Euler formula, watertightness, manifold invariants). No "no panic" tests.
- **References:** Relevant commits cite [#24] Yang 2025 and [#9] Cherchi 2020.

---

## Tolerance Audit (A14.3)

### Production code: CLEAN

All production-code tolerances derive from `units.rs` constants:
- `exact_mesh.rs` uses `TAU_WORK.sqrt()` (commented as `~1e-6`)
- `yang_integration.rs` uses `TAU_WORK`, `TAU_NORMALIZE_SQ`
- `types.rs` `BooleanOptions::for_scale()` uses `TAU_WELD_FACTOR`, `TAU_COINCIDENT`, etc.

### Test code: 1 violation found, fixed

- `yang_integration.rs:1837` — hardcoded `1e-15` in `yang_mesh_passthrough_watertight`
  degenerate area check. Replaced with `crate::units::TAU_NORMALIZE` (same value).
  **Fixed in commit 147ff42.**

### Test code: acceptable hardcoded values (not violations)

- `1e-6` in f32→f64 round-trip checks (lines 934-936, 1684) — these match f32
  precision (~7 decimal digits). No suitable semantic constant exists in units.rs.
  Adding one would be over-engineering.

---

## Workaround Detection (P9)

### No new workarounds found

Searched for: fallback, workaround, accept invalid, bypass, skip validation,
passthrough, cached mesh, hack, HACK, TODO revert, temporary.

**Hits categorized:**

| Pattern | Location | Verdict |
|---------|----------|---------|
| "fallback" | `waffle_kernel.rs:1127` | A15.2 enforcement — returns NotSupported, not a fallback |
| "fallback" / "temporary" | `waffle_kernel.rs:1180-1181` | Pre-existing deprecated S-H code, tracked in migration plan |
| "passthrough" / "bypassing" | `yang_integration.rs:237,685` | Legitimate: mesh boolean output used directly (Cherchi 2020) |
| "cached mesh" | `yang_integration.rs:688,695` | Legitimate: render mesh from exact boolean result |
| "P9: do not accept invalid" | `yang_integration.rs:677` | P9 enforcement comment — explicitly rejects invalid results |
| "fallback" | `stitch.rs:316,529` | Deprecated S-H pipeline code per A15.6 |
| "fallback" | `exact_mesh.rs:1339,1532` | GWN fallback for degenerate ray-cast — legitimate algorithm design |
| "fallback" | `tessellation/mod.rs:450-463` | Polygon face tessellation when analytic path unavailable — legitimate |
| "fallback" | `boolean/analytical.rs:280,288` | Documented polygon-clipping for incomplete analytical path — tracked |

**No P9 violations.** The Yang pipeline code (recent commits) correctly refuses invalid
results and propagates errors per A15.6.

---

## Changes Made

| Commit | Description |
|--------|-------------|
| 147ff42 | Replace `1e-15` → `TAU_NORMALIZE` in yang passthrough degenerate check (A14.3) |

---

## Summary

The 10 most recent commits are governance-compliant. The dev work (T-junction fix,
vertex dedup, deadline threading) is well-tested with proper numeric oracles. The
audit work (stale comments, tolerance centralization, spec alignment) was thorough.

**One minor A14.3 violation found and fixed** — a hardcoded `1e-15` in a test that
should use the `TAU_NORMALIZE` constant from `units.rs`.

**No P9 violations, no hack-to-green patterns, no fallback paths in new code.**

Test results: 998 pass, 28 ignored, 0 failures. Clippy clean. Format clean.
