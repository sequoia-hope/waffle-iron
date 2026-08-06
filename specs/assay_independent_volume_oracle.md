# SPEC — an independent geometric oracle for the assay corpus

**Status:** DESIGN (pre-code). Author: 2026-08-06. Vehicle: `crates/test-harness`.
Motivated by the 2026-08-06 review; see §2 for the measurement that prompted it.

---

## 1. The claim under examination

The kernel's headline property is **0 WRONG**: every failure is loud, no case
silently returns a wrong solid. That claim is only as strong as the WRONG
detector. This spec measures the detector and closes its largest hole.

## 2. The measured gap

Live assay, 2026-08-06, `--release`, 312 cases, 342 s:
**261 SUPPORTED_CORRECT / 0 WRONG / 47 ERROR / 0 TIMEOUT** (+2 UNSUPPORTED
coplanar, 1 UNSUPPORTED curved-profile, 1 EXPECTED_ERROR).

What a SUPPORTED_CORRECT verdict actually rests on (`assay_kv2.rs:340-400`):

| Oracle | Kind | Coverage of the 261 |
|---|---|---|
| watertight / 2-manifold mesh checks | topological | 261 |
| `mesh_euler_characteristic` vs `euler_target` | topological | 261 |
| `minimum_triangle_count` | structural | 261 |
| `volume_magnitude` (order of magnitude vs scale) | weak geometric | 261 |
| `volume_monotonicity` (direction per op) | weak geometric | 261 |
| `strict-validation` loop-vertex-on-surface | **per-vertex** geometric | 261 |
| **`expected_volume` (absolute, kernel-independent)** | **global geometric** | **90** |

Coverage of the absolute-volume oracle, by series:

| Series | total | CORRECT | absolute-volume oracle | ERRORs |
|---|---:|---:|---:|---:|
| C (curated) | 118 | 102 | **90 (88 %)** | 15 |
| F (fuzz) | 94 | 85 | **0** | 6 |
| R (random/revolve) | 100 | 74 | **0** | 26 |

**159 of the 261 passing verdicts carry no global geometric check** — the whole
F and R series. The risk profile is inverted: the two series with *zero* absolute
oracle are the two carrying 32 of the 47 known failures, i.e. the hardest
geometry in the corpus is the least verified when it passes.

### 2.1 What the existing oracles cannot see

`strict-validation` proves every loop vertex lies on *its own face's* analytic
surface. It does not prove the **right faces survived**. A boolean that keeps a
wrong patch — wrong side, dropped cavity, extra material — but still closes into
a watertight 2-manifold solid with the target Euler characteristic and a
plausible volume magnitude is invisible to the entire oracle set above, on 159
cases. That failure mode is precisely the "silently-wrong" class P9/P10 exists
to prevent.

### 2.2 Precedent — this exact blind spot has paid out once already

Compiling `kernel-v2/strict-validation` into the release assay (2026-07-28)
moved **three** cases that had been scoring SUPPORTED_CORRECT — F0083, R0027,
R0099 — to loud ERROR. They were always broken; the ledger could not say so.
The CORRECT bucket has demonstrably held defects before, and the thing that
found them was an oracle upgrade, not a tail fix.

The counter-evidence deserves its own line: on the 90 cases that *do* carry
`expected_volume`, it has never fired. That is real but it is a **C-series**
statistic, and the C-series is the curated, mostly axis-aligned, mostly
rectangular family. Extrapolating it to the F/R populations is the same
tail-slice-as-distribution error this campaign has logged twice (the 38-vs-16
double-count; the `n_moved=1` claim read off `tail -12`). The F/R risk is
**unmeasured**, not low.

## 3. Design

**Principle: the oracle must be independent of the code under test.** The
boolean is under test; the primitive constructors (extrude/revolve of a sketch)
are not. So the oracle may use the kernel to build *operands in isolation*, and
must not use it to combine them.

### 3.1 Expected set

For a case with ops `o_1 … o_N` from `<id>.meta.json` and `<id>.waffle`:

1. **Operand extraction.** For each `o_k`, synthesize a single-feature document
   — the op's own sketch plus one **boss** extrude/revolve (never a cut; a cut
   is re-authored as its tool body with the same profile, plane and depth) — and
   build it through the ordinary loader. Yields operand mesh `M_k`. No boolean
   is performed. The sketch is taken **verbatim from the `.waffle`'s solved
   entity positions**, not re-derived from `profile_size`, so no generator
   convention is assumed. (F0001 confirms the shape: `entities` + a
   `solved_positions` map + `solved_profiles`.)
2. **Composition.** Membership of a point `p` is folded over the op sequence:
   `in := false; for k: in = (in ∨ p∈M_k) if !is_cut else (in ∧ ¬p∈M_k)`.
   This mirrors the engine's own auto-union / cut semantics and nothing else.

### 3.2 Volumes

Two independent numbers per case, both reported:

- **`V_exact_out`** — signed volume of the output mesh by the divergence
  theorem (sum of tetrahedra). Exact for the tessellation, no sampling error.
- **`V_grid_out` / `V_grid_expected`** — a stratified column scan along +Z:
  bin triangles by their (x,y) projection, then for each column of a
  `n×n` grid over the model AABB collect z-crossings, sort, and pair them into
  inside-intervals. Applied to the **output mesh** and to the **composed
  operand set** by the same code path, so the discretization error is common-mode
  and cancels to first order in the comparison.

Determinism (Test Philosophy): the grid carries a fixed irrational offset to
avoid vertex-coincident columns. No RNG, no system time.

### 3.3 Tolerance policy — the part that must not become a band

An oracle that emits a false WRONG is worse than no oracle. Therefore:

- Both sides are tessellated at an **oracle-specific fine tolerance**,
  independent of the corpus render tolerance (`clamp(scale·0.01, 1e-9, 0.1)`,
  which admits ~1 % chord error on curved profiles and is far too coarse here).
- The comparison band is **computed, not chosen**: the reported discrepancy
  must exceed the sum of (a) the measured grid-convergence residual — obtained
  by running the scan at `n` and `2n` and taking the difference — and (b) the
  chord-error bound implied by the oracle tessellation tolerance and the model's
  surface area. A case is flagged only when it exceeds that computed bound by a
  stated factor.
- **Scope stated honestly up front: this oracle detects a wrong SET, not a wrong
  TOLERANCE.** Set-level errors (wrong patch survival, dropped cavity, extra
  material) are percent-to-100 % volume errors. Micron-level positional defects
  are already the `strict-validation` per-vertex oracle's job. A flag from this
  oracle means "the boolean kept the wrong material", and that is exactly the
  class the current oracle set is blind to.

### 3.4 Validation of the oracle itself

14 currently-passing F-series cases are chains of axis-aligned rectangle
extrudes: **F0001–F0010, F0051, F0053, F0091, F0093**. For these the expected
volume is computable **exactly** by the existing, unit-tested
`gen_complexity::{tool_box, chain_volume}` box-CSG sweep. They are the oracle's
calibration set: the general engine must reproduce the exact answer on all 14
to within its own stated grid residual before any verdict it emits elsewhere is
trusted. They are deliberately *not* the deliverable — all 14 are two-op
box∪box (F0093 box∖box), the easiest geometry in the corpus, and proving them
correct proves nothing about the kernel. Their job is to prove the *oracle*.

## 4. Increments

1. **Engine + calibration.** Operand extraction, column-scan point-in-solid,
   divergence-theorem volume, composition. Unit tests on synthetic fixtures
   (box, box∪box, box∖box, cylinder against its closed form) plus the 14-case
   exact-box calibration. Read-only, no assay wiring. *Gate: all 14 reproduce
   `chain_volume` within the reported grid residual.*
2. **Sweep.** Run over all 261 CORRECT cases; report the discrepancy
   distribution; classify anything above the computed band. Still read-only —
   the assay verdicts do not change.
3. **Decide from the measurement.** If the sweep is clean, `0 WRONG` is
   promoted from a topological claim to a geometric one across the whole corpus
   and the result is recorded in the roadmap DoD. If it flags cases, each flag
   is anchored case-first (the standing discipline) before any code moves.

Increment 2's outcome is informative either way, which is the property this
work is chosen for: unlike a repair mechanism, an oracle cannot be "measured
out".

## 5. Risks

- **False WRONG.** The dominant risk, addressed by §3.3 and §3.4. If the
  calibration set does not reproduce exactly, the engine is wrong and no sweep
  runs.
- **Operand re-authoring drift.** Re-authoring a cut as its tool body could
  diverge from what the engine actually cut with. Mitigation: the sketch comes
  from the `.waffle` verbatim, and the extrude parameters come from the same
  feature record the engine reads.
- **Coverage.** Revolve operands (108 ops) and gear profiles (123 ops) go
  through the same path, but a revolve tool for a *cut* is the least-exercised
  re-authoring case; if it cannot be made faithful, those cases are reported as
  NOT-COVERED rather than silently passed. Coverage is stated as a number in the
  sweep report, never implied.
