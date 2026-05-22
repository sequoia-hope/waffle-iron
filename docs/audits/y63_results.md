# Y63 Cycle Results — PR-A SHIPPED, PR-B ABORTED at P10 gate

## Status: partial cycle. 1 of 2 latent bugs fixed.

## Cycle goal

Fix two latent bugs exposed by Y62 (commit `db657fd`):
- Bug 1: `test_yang_subtract_face_geometry_complete` — face stored_normal mismatches arena loop Newell (dot=-0.447)
- Bug 2: `k1_cyl_minus_enclosed_box_volume` — volume 6.440 vs expected 5.783

## PR-A: SHIPPED — `make_tetra_solid` test fixture fix

**Diagnosis confirmed**: `make_tetra_solid` at `yang_integration.rs:2179-2186`
assigned a dummy `Vector3::new(0.0, 0.0, 1.0)` to ALL 4 tetra face normals,
regardless of geometric orientation. The fixture comment ("the exact normal
doesn't matter for this test") was provably false after Y62.

**Fix**: Derive each face's `stored_normal` from Newell of its outer loop walk,
and its origin from the loop's centroid. Same pattern as `revolve_face` cap
derivation (`waffle_kernel.rs:1613-1623`). Inline loop walk; no new helpers.

**Outcome**:
- `test_yang_subtract_face_geometry_complete` flipped FAIL→PASS
- `test_yang_face_geometry_fallback_valid_normal` ALSO flipped FAIL→PASS (a
  pre-Y62 baseline failure, opportunistically resolved by the same fix —
  both tests exercise tetra fixtures)
- Kernel count: 1250/34/43 (matches pre-Y62 baseline; net count unchanged,
  composition shifted by +2 GREEN / 1 outstanding RED = k1)
- F0020 oracles unchanged: all 6 PASS

## PR-B: ABORTED at P10 gate — plan's diagnosis was wrong

**Plan hypothesis**: Add reconciliation at `result_topology_to_waffle_solid`
(`yang_integration.rs:245-263`) to flip `Planar.normal` when `Newell · plane.normal < 0`,
fixing Bug 2.

**Diagnostic empirical result** (via `YANG_NORMAL_DIAG=1` env-gated eprintln in
PR-B step 1, P10 abort gate):

With `YANG_BOOLEAN=1` forced, k1's Yang-mode output shows the expected pattern:
```
face_idx=0 source=A face=0 dot=1.000   ← A face top cap
face_idx=1 source=A face=0 dot=-1.000  ← split-A face with reversed walk
face_idx=2 source=A face=1 dot=1.000
face_idx=3 source=A face=1 dot=-1.000  ← split-A face with reversed walk
face_idx=6 source=A face=1 dot=1.000
face_idx=7 source=B face=2 dot=-1.000  ← B-face becoming hole wall
face_idx=8 source=B face=3 dot=-1.000  ← B-face becoming hole wall
face_idx=9 source=B face=4 dot=-1.000  ← B-face becoming hole wall
face_idx=10 source=B face=5 dot=-1.000 ← B-face becoming hole wall
```

4 B-faces + 2 split-A faces show `dot=-1` — empirical confirmation that
Yang's topology_extract reverses walks for B-faces (via `tri.flipped`) while
preserving operand B's surface_map normal as-is. The reconciliation would
correctly negate these `plane.normal` fields.

**BUT**: k1 in default mode (no `YANG_BOOLEAN=1`) does NOT route through
Yang. The diagnostic eprintln NEVER FIRED for default-mode k1 (verified
with `--nocapture`). This is because `yang_boolean_from_solids` at
`yang_integration.rs:637-640` returns `NotSupported("not enabled")` unless
`YANG_BOOLEAN=1` is set, causing `do_boolean` to fall through to LEGACY
boolean (S-H clipping / polygon-clipping pipeline).

The default-mode k1 failure surfaces from Y62's `tessellate_planar_face_bounded`
fix interacting with LEGACY-boolean-produced `face_geometry`, NOT Yang.
PR-B's fix would not touch the legacy code path.

**P10**: per Constitution, "if a plan's diagnosis turns out to be wrong, the
agent must abort that fix and report what it learned — not improvise an
alternative." The diagnosis localized the fix to `result_topology_to_waffle_solid`,
but k1's default-mode failure routes through legacy assembly. ABORT.

**With YANG_BOOLEAN=1**: k1 produces volume 0.167 (Yang is incomplete for
this scenario at higher levels — topology issues, not stored_normal). Y63's
reconciliation would not fix this either, since `mesh_volume` integrates
tri winding (which is determined by `tessellate_planar_face_bounded` from
the polygon walk, not from `stored_normal`).

## Net cycle outcome

| Metric | Pre-Y62 | Post-Y62 (HEAD) | Post-PR-A (this cycle) |
|---|---|---|---|
| Kernel test count | 1250/34/43 | 1248/36/43 | 1250/34/43 |
| `test_yang_subtract_face_geometry_complete` | PASS | FAIL | PASS |
| `test_yang_face_geometry_fallback_valid_normal` | FAIL | FAIL | PASS |
| `k1_cyl_minus_enclosed_box_volume` | PASS | FAIL | FAIL |
| F0020 6 oracles | mixed | all PASS | all PASS |
| yang_fast | 12/157 | 12/157 | (not re-run; expected unchanged) |

Net: 1 latent bug fixed (Bug 1 + opportunistic baseline fix); 1 latent bug
remains (Bug 2, k1 volume; banked for follow-up). Test count restored to
pre-Y62 baseline.

## Findings banked for follow-up cycles

1. **k1 default-mode bug is in legacy boolean output, not Yang**: A separate
   cycle is needed to either (a) fix `tessellate_planar_face_bounded` to
   handle legacy-produced face_geometry differently, (b) audit legacy
   boolean's face_geometry assignment to ensure stored_normal matches
   polygon walk, or (c) move the reconciliation to a post-boolean,
   pre-tessellation layer that applies to BOTH Yang and legacy outputs.

2. **YANG_BOOLEAN=1 k1 produces volume 0.167**: Yang itself is incomplete
   for cyl-minus-enclosed-box. This is a separate (likely larger) Yang
   completeness issue, independent of the stored_normal reconciliation.

3. **Yang's reconciliation is still a real defect**: The diagnostic empirically
   confirms `dot=-1` for 6/9 Planar faces in Yang's k1 output. When Yang
   becomes complete for more scenarios, this reconciliation will become
   load-bearing for correctness of those scenarios' face_geometry consumers
   (rendering, picking, secondary booleans). Banked for that future cycle.

4. **Adversarial tests dropped**: `k1b_chained_cyl_minus_two_boxes_volume`
   and `k1c_intersect_two_boxes_normals_consistent` were drafted but
   discarded — both fall back to legacy polygon-clipping (Yang doesn't
   support chained subtract or two-box intersect currently), so they don't
   exercise the targeted code path.

## Methodology note

The P10 abort gate worked exactly as designed. The diagnostic eprintln was
added BEFORE the reconciliation implementation, ran the failing test, and
empirically refuted the localization of the fix to `result_topology_to_waffle_solid`
for the test we were trying to fix. This is the canonical use of P10:
plans are cheap, hack-driven implementations are expensive.

Compare with the Y61 → Y62 sequence, where the localization to
`tessellate_planar_face_bounded` was correct for F0020 but the function's
contract was masking upstream defects — there the regressions were
LATENT BUGS (per `feedback_regressions_can_be_unmasked_latents.md`). Here
the regression is NOT a latent bug; it's that the proposed fix's site
isn't reached. Different category; different response (ABORT vs SHIP).

## Verification

```bash
# PR-A RED→GREEN
cargo test -p kernel --lib test_yang_subtract_face_geometry_complete
# expect: PASS

# Net kernel state
cargo test -p kernel --lib 2>&1 | tail -3
# expect: 1250 passed; 34 failed; 43 ignored (matches pre-Y62 baseline)

# F0020 oracles
YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- \
  spotlight_f0020_oracles --ignored --nocapture
# expect: all 6 PASS
```
