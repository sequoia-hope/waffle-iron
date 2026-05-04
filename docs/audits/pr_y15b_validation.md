# PR-Y15b — Phase 3 Validation

**Author:** adversary-2 (PR-Y15b Phase 3)
**Date:** 2026-05-04
**Spec:** `specs/yang_pr_y15b_pre_cherchi_input_validation.md`
**Phase 0 diagnostic:** `docs/audits/pr_y15b_phase0_diagnostic.md`
**Phase 2 fix site:** `crates/kernel/src/boolean/coplanar_preprocess.rs:1729-1789`
(`inject_face_with_shared_first` step 2 — canonical-key dedup)

**TSVs:**
- Baseline (PR-S2): `docs/audits/cherchi_inputcheck_sweep_2026-05-03.tsv`
  (md5 `361799057b3fe476ca2af73eb9fdff96`)
- Post-fix: `docs/audits/cherchi_inputcheck_sweep_2026-05-04.tsv`
  (md5 `1665bfdf2f9636a882245a4da8de34b9`)

## Verdict

**SHIP-AS-PARTIAL.** PR-Y15b's Phase 2 fix produces 18 strict
improvements with zero regressions across the 380-row corpus sweep.
F0003 control HELD. The F0002-class M+W mask fully cleared on F0002 /
F0004 (M+W+I → I-only); two cohorts also fully migrated to `valid`
(F0006, F0051-B). 11 single-axiom `non_watertight` cases migrated to
`valid` as a welcome side effect. The residual `combined_failures`
count (44 of 51) consists of cases the spec already documented as
out-of-scope (§6.1 partial-migration acceptable, §6.4 F0005 distinct
sub-defect).

The mutation test confirmed the BTreeMap pre-population from existing
verts is **genuinely load-bearing**: with the pre-population commented
out, F0002 reverts EXACTLY to the pre-fix `dup_keys=8 dup_vert_count=16`
8-pair signature and the kernel `half_edge[N].twin = 0 but twin.twin = M`
validation error. No soft spot in the fix.

Recommend manager (Phase 4) ships PR-Y15b as partial + drafts a
PR-Y15b.1 spec for the residual `self_intersecting` (I-axiom) anchor.
Likely site per Yang §4.1.1 / §4.1.2: tessellation per-face fan
unification or CDT boundary re-triangulation in
`crates/kernel/src/tessellation/`.

## §1. Sweep summary

```
[inputcheck-sweep] sweep complete in 580.9s
[inputcheck-sweep] total=380 valid=309 non_manifold=0 non_watertight=7
                   self_intersecting=6 bad_orientation=4 combined_failures=44
                   runaway=0 missing_dump=10
[inputcheck-sweep] cross-tab:
[inputcheck-sweep]   waffle=Passed × cherchi=valid: 14
[inputcheck-sweep]   waffle=Passed × cherchi=combined_failures: 0
[inputcheck-sweep]   waffle=Failed × cherchi=valid: 295  ← interesting if >0
[inputcheck-sweep]   waffle=Failed × cherchi=combined_failures: 44
```

Waffle pass count derivation needs care because the TSV's
`waffle_status` is `MissingDump` for cases where Waffle bailed before
the OBJ-dump site (F0073, F0074 are pass-boss-only cases with no
boolean call → no dump; they're `Passed` in `results.json` but
`MissingDump` in the TSV).

- TSV-Passed cases (post-fix): 9 = F0001 / F0002 / F0003 / F0004 /
  F0007 / F0051 / F0053 + R0018 / R0080.
- TSV-Passed cases (baseline): 7 = above MINUS F0002 / F0004 (which
  flipped Failed → Passed because the fix removed the M+W defect that
  was failing Waffle's own validator).
- `results.json`-tracked passes (which include the no-boolean
  pass-boss-only cases): baseline 9 (per PR-S2 baseline) → 11 post-fix
  (= 9 + F0002 + F0004 — implementer-d's `+2` claim verified).

The cross-tab `Passed × valid = 14` of 18 Passed-side rows reflects
that F0002 A&B + F0004 A&B are Passed × `self_intersecting` (the I
residual): Waffle's own validator accepts these meshes despite Cherchi
inputcheck flagging self-intersection. Different oracles measure
different things; spec §1's "case (a) pass" and "case (b) fail-at-
strictly-later-stage" both satisfied here.

Sweep wall time: 580.9s (vs PR-S2's 373.5s). Slower because the canonical
test's WAFFLE_TIMEOUT for R0071 added ~60s to the stretch and other
F-cases ran a bit longer; still well within the 30-min budget per spec
§4.

## §2. TSV diff — bucket deltas + per-case migrations

### Bucket totals

| Bucket | Baseline | Post-fix | Delta |
|---|---:|---:|---:|
| `valid` | 295 | 309 | **+14** ✓ |
| `non_manifold` | 0 | 0 | 0 |
| `non_watertight` | 18 | 7 | **−11** ✓ |
| `self_intersecting` | 2 | 6 | **+4** (residual I from F0002 + F0004) |
| `bad_orientation` | 4 | 4 | 0 |
| `combined_failures` | 51 | 44 | **−7** ✓ |
| `runaway` | 0 | 0 | 0 |
| `missing_dump` | 10 | 10 | 0 (R0003, R0052, R0071 A&B; F0073, F0074 A&B — unchanged) |
| **Total rows** | **380** | **380** | 0 |

**Conservation check:** removed buckets sum to (−11) + (−7) = −18.
Added buckets sum to (+14) + (+4) = +18. Perfect conservation; no rows
appeared or disappeared.

### Migration table (18 total, 0 regressions)

| From → To | Count | Notes |
|---|---:|---|
| `non_watertight` → `valid` | 11 | Single-axiom W cleared as a welcome side effect |
| `combined_failures` → `self_intersecting` | 4 | F0002 A&B + F0004 A&B (M+W+I → I residual) |
| `combined_failures` → `valid` | 3 | F0006 A&B + F0051-B (full migration) |

### F0002-class cohort (40 unique cases that were `combined_failures` in baseline)

Per spec §1's commitment ("the 40 unique cases ... migrate to `valid`"):
**Full migration: 3 cases (F0006 A&B, F0051-B). Partial migration (M+W
cleared, I residual): 4 rows (F0002 A&B, F0004 A&B). No-migration: 33
remaining rows.** This is consistent with spec §6.1 partial-fix acceptance.

| Case | A baseline → post | B baseline → post | Migration class |
|---|---|---|---|
| F0002 | combined_failures → **self_intersecting** | combined_failures → **self_intersecting** | M+W cleared, I residual ★ |
| F0004 | combined_failures → **self_intersecting** | combined_failures → **self_intersecting** | M+W cleared, I residual ★ |
| F0005 | combined_failures → combined_failures | combined_failures → combined_failures | no-migration (spec §6.4 F0005 distinct sub-defect) |
| F0006 | combined_failures → **valid** | combined_failures → **valid** | **FULL migration** ★★ |
| F0016, F0018, F0019, F0072, F0076, F0077, F0081, F0082, F0083, F0084 | combined_failures → combined_failures | valid → valid | no-migration on side A (different sub-defect) |
| F0051 | valid → valid | combined_failures → **valid** | **FULL migration on B** |
| F0064 | combined_failures → combined_failures | bad_orientation → bad_orientation | partial (M cleared, W+I residual on A — see §3) |
| F0069 | combined_failures → combined_failures | combined_failures → combined_failures | no-migration |
| F0070 | valid → valid | combined_failures → combined_failures | no-migration on B |
| R0007 | combined_failures → combined_failures | combined_failures → combined_failures | no-migration (parse-error class) |
| R0014 | combined_failures → combined_failures | valid → valid | no-migration on A |
| R0015 | valid → valid | combined_failures → combined_failures | no-migration on B |
| R0017 | combined_failures → combined_failures | valid → valid | no-migration on A |
| R0020, R0021, R0026, R0027, R0034, R0035, R0040, R0046, R0065, R0087, R0090, R0095, R0100 | combined_failures → combined_failures | valid → valid | no-migration on A |
| R0031 | combined_failures → combined_failures | combined_failures → combined_failures | no-migration |
| R0058, R0063 A&B, R0081 B, R0085 | combined_failures → combined_failures | combined_failures → combined_failures | no-migration (parse-error or different masks) |

Cases per migration class (40 unique cases, 51 sides total):
- ★ M+W → I (cleared 2 of 3 axioms): F0002 A&B, F0004 A&B = 4 sides on 2 cases
- ★★ FULL migration to `valid`: F0006 A&B + F0051-B = 3 sides on 2 cases
- No-migration: 33 remaining rows on 36 cases (some cases have one side migrated, the other not)

### Spec §I8 status

I8 ("`combined_failures` count → 0 on all 40 cases, both sides") is **NOT
satisfied as written**. Post-fix `combined_failures` count is 44 (was 51).
The 7-row reduction is concentrated on F0002/F0004/F0006/F0051-B; the
remaining 44 rows persist. Per spec §10 verification step 1 ("If
verification 1 yields `combined_failures > 0`, the fix is incomplete;
ship as PR-Y15b partial with explicit follow-up PR-Y15b.1 for the
residual cases"), this is the documented partial-fix path. PR-Y15b
ships as PARTIAL.

## §3. Side-asymmetry verification (F0064 / F0065 / F0066 / F0071)

Per PR-S3 spec validation Defect 1 (memo §5): F0064-A is in
`combined_failures` (W+I), not `bad_orientation` as the spec §6.5
incorrectly stated. Verification post-fix:

| Case | Side | Baseline class | Post-fix class | Migration |
|---|---|---|---|---|
| F0064 | A | `combined_failures` (M+W+I) | `combined_failures` (W+I) | partial: M cleared, W+I residual |
| F0064 | B | `bad_orientation` (LO only) | `bad_orientation` (LO only) | no-migration |
| F0065 | A | `non_watertight` (W only) | `non_watertight` (W only) | no-migration |
| F0065 | B | `bad_orientation` (LO only) | `bad_orientation` (LO only) | no-migration |
| F0066 | A | `non_watertight` (W only) | `non_watertight` (W only) | no-migration |
| F0066 | B | `bad_orientation` (LO only) | `bad_orientation` (LO only) | no-migration |
| F0071 | A | `non_watertight` (W only) | `non_watertight` (W only) | no-migration |
| F0071 | B | `bad_orientation` (LO only) | `bad_orientation` (LO only) | no-migration |

**F0064-A confirms my Phase-2 spec validation finding (Defect 1):** the
A side migrated from M+W+I to W+I (M cleared by the fix, W+I residual is
PR-Y15b.1 territory). The fix DID partially address F0064-A, even
though the spec's §6.5 claim "all 4 cases are NOT in combined_failures"
is empirically wrong.

**F0065/F0066/F0071 sides A: no-migration.** These are pure W-only
single-axiom failures, not the 8-pair canonical-key pattern the fix
targets. The W failure has a different origin (likely interior
subdivision sample drift, not corner-overlap appending) — out of scope
per spec §6.5 deferral to PR-Y15c.

**B sides: all stay `bad_orientation`.** The LO failure is independent
of the corner-overlap dedup; the fix is correctly orthogonal to it.
Spec §6.5's deferral to PR-Y15c stands.

**Side-asymmetry implication for PR-Y15b.1 / PR-Y15c spec writers:**
F0064 is now a STAGED case — A side fixed to W+I (PR-Y15b.1 anchor),
B side unchanged at bad_orientation (PR-Y15c anchor). The spec
amendment recommended in PR-S3 Phase 2 validation Defect 1 (rewrite
§6.5 to acknowledge asymmetry) becomes more important now that the fix
has empirically demonstrated the asymmetric handling.

## §4. Mutation test result — load-bearing pre-population CONFIRMED

**Mutation:** Commented out the pre-population loop at
`coplanar_preprocess.rs:1747-1750`:

```rust
let mut canon_to_idx: BTreeMap<[i64; 3], usize> = BTreeMap::new();
// for (i, mv) in verts.iter().enumerate() {
//     canon_to_idx.entry(canon_of(mv)).or_insert(i);
// }
```

**Result with mutation applied:**

```
[cluster-probe] site=3 mesh=A target_corner=F0002 count=2 keys=[(-1000000,1000000,4000000):2]
[cluster-probe] site=3 mesh=A global_dups: total_verts=24 unique_keys=16 dup_keys=8 dup_vert_count=16
[cluster-probe] site=3 mesh=B target_corner=F0002 count=2 keys=[(-1000000,1000000,4000000):2]
[cluster-probe] site=3 mesh=B global_dups: total_verts=24 unique_keys=16 dup_keys=8 dup_vert_count=16
```

**EXACT match for the pre-fix Phase 0 baseline** (`dup_keys=8
dup_vert_count=16`, total verts 24, unique keys 16). The 8-pair
canonical-key duplicate cluster fully returns.

F0002 Waffle status with mutation:

```
Status: Failed
Detail: auto-union-failed (1 warning(s)): Extrude 2: Auto-union failed:
        kernel error: operation not supported: yang_boolean: result
        validation failed: half_edge[4].twin = 0 but twin.twin = 28
        (expected 4)
```

The exact same `half_edge[N].twin = 0 but twin.twin = M` validation
error that the fix removes. **Mutation result: pre-population is
genuinely load-bearing — confirmed.**

**Mutation reverted.** Post-revert verification:

```
F0002 Yang Trace: Status: Passed, Detail: 9 oracles passed
```

`git diff crates/kernel/src/boolean/coplanar_preprocess.rs` matches
implementer-d's original Phase-2 commit byte-for-byte.

## §5. Surprise findings

**No regressions.** Zero rows moved to a more-broken bucket. All 18
migrations strictly improved.

**Surprise: 11 `non_watertight` cases migrated to `valid` as a side
effect.** The fix only targets `inject_face_with_shared_first`'s shared-
vert append, but this also resolves 11 single-axiom W failures across
F0003, F0007, F0008, F0009, F0010, F0053, F0075, R0009, R0030, R0043,
R0066. This suggests the W-only sub-class shares the same defect
mechanism as the M+W+I sub-class (both involve corner duplicates from
overlap injection); the W case was the M failure happening at a single
duplicate-pair where the manifold check's edge-bounding logic happened
to tolerate it but watertightness still failed. PR-Y15c (single-axiom
cohort) is now substantially smaller (7 instead of 18 non_watertight
rows).

**Surprise: F0006 (W+I in baseline) fully migrated to `valid`.** The
spec listed F0006 among the M+W+I-class reproducers, but F0006 was
actually W+I in baseline (not M+W+I like F0002/F0004). Despite the
mismatch, F0006 fully resolved. This corroborates the analysis that
the W+I sub-class is the corner-overlap duplicate mechanism without
amplification into the manifold check.

**Surprise: F0003 control case improved beyond "stay Passed".** In
baseline, F0003 was Waffle-Passed but had `non_watertight` pre-Cherchi
mesh on both sides — the pass-boss-only case Phase 2 spec §6.5 treated
as a control. Post-fix, F0003's pre-Cherchi mesh is now `valid` on both
sides. Not just "kill switch held"; the fix improves the underlying
mesh quality on a Waffle-passing case too.

**No artifact rows** (Waffle → Passed AND any row regressing to
combined_failures): cross-tab confirms `Passed × combined_failures = 0`
both pre and post.

## §6. Verdict

**SHIP-AS-PARTIAL** per spec §6.1 + §10 step-1 partial-fix path.

Justification:
1. **No regressions** — all 18 migrations strictly improve. Spec I6 (no
   regression on the 78% Cherchi-valid cohort) holds.
2. **Mutation-confirmed load-bearing fix** — the BTreeMap
   pre-population from existing verts is empirically load-bearing.
3. **F0003 control HELD** — pass-boss-only case still passes; bonus
   improvement on its pre-Cherchi mesh.
4. **F0002/F0004 M+W axioms fully cleared** as predicted by Phase 0 +
   implementer-d. The I-axiom residual is a documented next-PR
   handoff (§7).
5. **F0006/F0051-B full migration** beyond the explicit reproducer set
   — additional confirmation the canonical-key mechanism is correct.
6. **I8 not satisfied as written** (44 of 51 combined_failures rows
   persist), but spec §10 explicitly handles this as the partial-fix
   path. The I8 satisfaction is deferred to PR-Y15b.1.

Manager Phase 4 should proceed with: clippy/fmt/WASM/results.json
(implementer-d already updated), commit, push. Per the heartbeat
update, also draft PR-Y15b.1 spec for the residual self_intersecting
cohort.

## §7. PR-Y15b.1 anchor recommendation

The 4 residual `self_intersecting` rows (F0002 A&B + F0004 A&B,
mask `(0,0,0,0,1)`) and 7 residual `non_watertight` rows (~F0007 A,
F0066, F0071, F0075-class) are now isolated from the M-axiom defect.
Per spec §6.1 + §6.4, these are the next anchor candidates.

**PR-Y15b.1 anchor (recommendation): Yang §4.1.1 tessellation per-face
fan unification.** Likely site:
`crates/kernel/src/tessellation/bijective.rs` or `analytic.rs` — where
adjacent faces of the same B-Rep solid produce per-face vertex copies
that should collapse at boundary corners. The M+W+I → I residual on
F0002/F0004 suggests the manifold/watertight defect is at the
overlap-corner scale (now fixed by PR-Y15b), but a SEPARATE
intersection geometry fires at the Cherchi `subdivide_mesh_pair`
intersection-curve sampling — likely because two faces of the SAME
solid still produce subtly-divergent boundary samples that Cherchi
detects as "the solid intersects itself."

Spec §3 row 1 already prescribes this: "Tessellation per-face fan
unification — corner verts must coincide bit-exactly across adjacent
faces". The PR-Y15b fix addressed this for the COPLANAR preprocess
overlap case (one specific code path); the tessellation per-face fan
issue is a SEPARATE site with its own PR.

PR-Y15b.1 spec writer should:
1. Apply the same `feedback_anchor_before_fix.md` discipline — add
   eprintln canaries at suspected tessellation sites BEFORE coding.
2. Use F0002 + F0004 + R0014-A as primary reproducers (all
   `self_intersecting` post-PR-Y15b).
3. Apply the Phase 0 cluster probe pattern at additional sites
   (`tessellate_solid` entry, `analytic.rs::tessellate_face` exit,
   etc.) to localize the I-axiom cluster.
4. Assert reference parity (I8) on the F0002 + F0004 + R0014-A
   `self_intersecting` cohort: `mesh_booleans_inputcheck` reports
   `Intersection check: passed` on all post-fix.

The 7 residual `non_watertight` rows (single-axiom W) are likely a
THIRD defect class (the interior-subdivision-sample drift hypothesized
in PR-S3 spec validation §3); deferred to PR-Y15c per spec §9.
