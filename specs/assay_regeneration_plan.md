# Assay Regeneration Plan

## Status: DONE (implemented 2026-03-17)

Sprint K (cylinder-minus-box boolean) landed. Assay needs regeneration to pick
up the new analytical path. Also: fix coplanar bias in the generator.

## Regeneration Steps

```bash
# 1. Regenerate the .waffle corpus
cargo run -p test-harness --bin assay_gen -- --seed 42 --count 100 --output app/tests/cases/assay

# 2. Run the full kernel assay
cargo test -p test-harness --test assay_randomized randomized_assay_full_kernel -- --ignored
```

## Problem: Coplanar Sketch Plane Bias

**All R-series cases use a single random plane shared across all 2-3 operations.**

In `crates/test-harness/src/assay/gen.rs` (~line 150-170), one `random_plane()`
call is made per case and reused for every operation. This means:

- Within a case, all extrudes/revolves are coplanar
- Booleans always hit the Z-aligned fast path after frame rotation
- Cross-plane booleans (the harder case) are only tested by 15 featured cases
  (F0011-F0025)

### Proposed Fix

Modify R-series generation in `gen.rs` to support per-operation planes:

1. **50/50 mix strategy** — half of R-series cases keep the shared plane (tests
   coplanar stacking, which is a real CAD workflow), half get per-operation
   `random_plane()` calls (tests cross-plane booleans)

2. **Implementation** — around line 348 where ops are generated in the loop:
   - Add a boolean flag `multi_plane` set by `rng.gen_bool(0.5)`
   - If `multi_plane`, call `random_plane()` again for each subsequent operation
   - Store per-op plane in `AssayMeta` (already has `plane_origin`/`plane_normal`
     optional overrides for this, used by F0011-F0015)

3. **Angular separation floor** — for multi-plane cases, optionally reuse
   `generate_well_separated_normals()` (requires >= 30 deg separation) to avoid
   near-degenerate plane pairs that produce numerically unstable SSI curves

### Files to Change

| File | Change |
|------|--------|
| `crates/test-harness/src/assay/gen.rs` ~line 150-170 | Add per-op plane logic |
| `crates/test-harness/src/assay/gen.rs` ~line 348 | Apply multi_plane flag in op loop |

### Expected Impact

- Better coverage of frame rotation + non-aligned SSI paths
- More assay failures initially (exposing real gaps), but more representative
  of real CAD workflows where features are on different planes
- Sprint K's cylinder-minus-box fix should reduce boolean-not-supported from 7 to <=4

## Reference

- Generator: `crates/test-harness/src/assay/gen.rs` (1358 lines)
- Runner: `crates/test-harness/src/assay/randomized_runner.rs`
- Binary: `crates/test-harness/src/bin/assay_gen.rs`
- Corpus output: `app/tests/cases/assay/`
