# #178 — sub-resolution coplanar-gap STOP (C0111/C0113-F1: silent sliver-wall dissolve)

**Task:** #178 (endgame continuous — silent-wrong → loud STOP, the sanctioned
P10 posture). **Findings:** C0111-F1 / C0113-F1
(`specs/assay_junction_scenario_corpus.md` §Baseline, task #176). **Blocks:**
the corpus-wide 0-WRONG ratchet (#174) — C0111/C0113 are the last live
SUPPORTED_WRONG cases.

## 1. Root cause (CONFIRMED by probe, 2026-07-17)

Stage-0's near-coplanar cross-pair detection
(`crates/yang-rs/src/boolean/coplanar_scan.rs::near_coplanar_band`) flags two
parallel planes as ONE model plane when their orientation-aligned unit-normal
offsets agree within `band = max(TAU_MODEL, scale·TAU_WORK)`. The §4.5.5
overlay then builds identical meshes on the shared canonical plane — which
**silently dissolves any volume between the two real planes**. For a designed
sub-resolution wall this converts topology with no error: χ 0→2, the
watertight/volume oracles pass (the wall's volume is below their tolerance),
and only the χ oracle catches it (SUPPORTED_WRONG).

Probe evidence (`YANG_COPLANAR_PROBE=1`, new `cross-pair` tag):

- **C0113** (s=1, ε=1e-7): pair (5,3), `gap=1.000e-7 ≤ band=1e-7` → detected
  → dissolved → χ=2 WRONG. The wall pair is A's +x face (d=−0.5) × the
  tool's wall face (d=−0.49999990000000005).
- **C0111** (s=1e-3, ε_rel=1e-5): pair (5,3), `gap=1.000e-8` → same dissolve.
- **C0112** (s=1e3, wall 1e-2 > band) → no pair → wall survives → CORRECT.
- **C0114/C0115** (zero-thickness by construction): all pairs `gap=0.000e0`
  (bit-exact planes) → the dissolve is the DESIGNED, correct §4.5.5 outcome.

The scan's own doc comment states the contract assumption:
`MIN_FEATURE_SIZE = 1e-6 guarantees genuinely distinct [features]` — i.e. a
model with two distinct parallel planes closer than the band is OUT OF
CONTRACT. Out-of-contract input must reject LOUDLY, never mutate silently
(P10; the R0091 trap class).

## 2. The discrimination line (measured; first attempt REFUTED by a real model)

Within the detection band the measured populations are:

| Population | gap | evidence |
|---|---|---|
| Bit-exact coplanar pairs (flush/stacked/zero-thickness — the mainstream §4.5.5 class) | exactly `0.0` | 211 of 318 detections in the corpus-wide gap survey (311 cases, 8-way parallel single-case probe runs; 101 cases reach the Stage-0 cross path) |
| Rounding twins (chained femto class) | ≤ `2.728e-12` (max: R0027 at scale ≈ 4944) | 107 detections |
| **Real producer residuals** (intended-coincident geometry from the app chain at mm scale) | `2.235e-10` | `bearing_recess_mm_regression` — the user's real 31mm bearing recess; the documented "~1.5e-10 residual at ~1e-2 model scale" class (yr27 Finding-2 completion) |
| Sub-resolution DISTINCT planes (a real interposed feature the overlay would dissolve) | C0111 `1e-8`, C0113 `1e-7` | the 2 WRONG cases |

**First attempt (REFUTED):** `gap > TAU_WORK·(1+scale)` (the
`is_relocation_coincidence` shape, ~1e-12 here) separated the corpus
populations with ≥40× margin — but the corpus only contains kernel-chained
femto noise. The REAL-model regression `bearing_recess_mm_subtract_succeeds`
(caught by the full tier) carries an intended-coincident pair at gap
2.235e-10, which that line wrongly rejected: real app-chain producer
residuals are ~2 orders above kernel femto noise.

**Criterion (calibrated):** a cross pair with `gap > band/100` is two
DISTINCT model planes. The line = 1% of the pair's own detection band
(absolute floor `TAU_MODEL/100 = 1e-9`; scales as `scale·TAU_WORK/100`
beyond scale 1e5) — a coincidence-authoring precision of 1% of the
resolution at which the model defines coincidence. Margins: weld side —
bearing residual 4.5× below, corpus femto ≥370× below; STOP side — C0111
10× above, C0113 100× above. No corpus gap lies between 2.7e-12 and 1e-8,
so corpus verdicts are identical under any line in that window; the
bearing fixture pins the line from below, the designed rungs from above.
Survey caveat: cases exceeding the 60 s survey cap were unmeasured; the
full-assay per-case diff (§5) is the authoritative gate.

## 3. Change

1. **Plumbing (shipped with the probe):** `near_coplanar_band` returns
   `(band, gap)`; `CrossCoplanarPair` carries `gap` (+ `sub_resolution`
   flag); `stage0_preprocess` probes every detected cross pair
   (`cross-pair` tag: faces, band, gap, surfaces).
2. **STOP:** in `stage0_preprocess`, after the intra-solid wall (existing
   precedence preserved) and before scope validation: the FIRST cross pair
   (deterministic (face_a, face_b) scan order) with `gap > band/100`
   raises the new typed
   `YangError::SubResolutionCoplanarGap { face_a, face_b, gap, band }`.
3. **Mapping:** kernel-v2 `map_yang_error` intentionally UNCHANGED — the new
   variant takes the default `KernelV2Error::BooleanFailed(display)` arm: a
   loud ERROR, **not** `UnsupportedCoplanar` (this is an input-contract
   violation, not a capability gap).

Non-goals (recorded): no tolerance welding of the wall (the R0091 trap); no
angular (tilt) discrimination — condition 2's `sin·ext ≤ band` admits femto
tilt only alongside offset agreement, and no corpus case exposes a tilt-only
analog; the coincident-CYLINDER pair path (`PairCylinder`, radius gap) has
the same theoretical hazard but no corpus case — goes to the triage ledger
as a named scenario requiring a corpus case per the coverage directive
BEFORE machinery.

## 4. Invariants

- **I1 (byte-identical outside the STOP window):** pairs with
  `gap ≤ band/100` (bit-exact, femto twins, producer residuals) take the
  overlay exactly as before.
- **I2 (total):** the STOP fires before any mesh/overlay work — no partial
  mutation, no half-built Stage-0 output.
- **I3 (deterministic):** first offending pair in the scan's (ia, ib) order.
- **I4 (loud, typed):** surfaces as a kernel ERROR with the gap and both
  face indices in the message; never `UnsupportedCoplanar`.

## 5. Measurement gate (all must hold before commit)

1. Unit red→green in `yang-rs` (`tests_unit/n178_subres_coplanar.rs`):
   C0113-mirror subtract → typed STOP; bit-exact flush subtract → `Ok`;
   femto-gap pair → `Ok`; producer-residual-gap (2e-10) pair → `Ok`;
   mm-scale C0111-mirror → STOP. Plus `bearing_recess_mm_regression`
   (real model, welds) and the yr27 pair (r=1e-10 welds / r=1e-8 STOPs).
2. Single cases: C0111, C0113 → ERROR (`SubResolutionCoplanarGap`);
   C0112, C0114, C0115, C0029–C0031, C0034 stay CORRECT.
3. Full categorized release assay: per-case diff vs the committed baseline
   (248C / 2W / 54E, WRONG = exactly {C0111, C0113} since #173 converted
   C0105/C0116) shows EXACTLY {C0111, C0113}: WRONG→ERROR; every other case
   category-identical. Post-#178 corpus = 248C / **0 WRONG** / 56E — the
   #174 ratchet can then bind corpus-wide.
