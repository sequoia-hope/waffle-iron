# KV9-F3 — output vertex identity (Stage-4 junction-duplicate collapse for unmoved vertices)

**Status:** spec (FIP Phase 1) — §2 Measured mechanism COMPLETE (2026-07-04).
**Change class:** bug fix (modeling-related). **Crates:** `yang-rs`
(`stage4_relocate_and_correct` step 3c), `kernel-v2` (un-quarantine only).

## 1. Goal

The boolean output B-Rep carries each geometric junction as ONE vertex.
Today the parallel cyl×cyl secant subtract (kv9 fixture r1=0.30, r2=0.22,
d=0.35) emits femto-twin vertex pairs at cylinder-seam junctions —
(0.13, 0, 1) vs (0.13, 5.39e-18, 1) at the tool seam and (0.3, 0, 0.4) vs
(0.3, −6.12e-18, 0.4) at the body seam — each pair bridged by a degenerate
Arc edge that the always-on G1 render gate rejects loudly
(`TessellationFailed("planar triangle collapsed at render precision")`,
2 kv9 tests quarantined `KV9-F3`; spec `kv2_cdt_triangulation_core` §6a,
`m8_shared_boundary_identity` §8b).

## 2. Measured mechanism (2026-07-04, `KV2_OUT_TWIN_PROBE` +
## `YANG_S4_TWIN_PROBE`)

1. Both twin pairs exist in yang's OUTPUT B-Rep (upstream of the kernel-v2
   assembler) and already in the Stage-4 mesh.
2. Stage-4 twin census on the fixture: both pairs have
   `moved=(false,false)`, `shared_tri=Some(_)`, and BOTH members carry a
   `vert_circle` conic-endpoint assignment on the same intersection
   circle. I.e. they are junction duplicates of one geometric point (the
   arrangement legitimately mints one vertex on the seam edge exactly and
   one from an adjacent chord crossing, ~5e-18 apart), both already within
   `TAU_WORK` of the exact circle, so the relocation pass moves NEITHER
   (`rho ≤ TAU_WORK` → not inserted into `moved`).
3. The §4.4.1(b) sub-feature merge (step 3c, spec
   `yang_n2_stage4_cdt_mesh_updating` N2-1) merges sub-`MIN_FEATURE_SIZE`
   edges of degenerate triangles — but its scan is restricted to triangles
   touching a `moved` vertex. Unmoved junction duplicates are invisible to
   it, so the sub-floor edge (5e-18 ≪ MIN_FEATURE_SIZE) survives through
   topology reconstruction as a degenerate Arc output edge.
4. This is exactly the population the step-(2) I6 weld DELEGATES to
   Stage-4 for curved inputs ("Stage-4 owns junction-duplicate collapse" —
   the bit-exact-only curved weld was a deliberate KV9 decision: welding
   at step 2 collapsed lens-tip seam edges). The delegation contract has a
   hole: Stage-4 only collapses duplicates it happened to move.

### 2a. Second measured layer (GREEN-phase re-measurement, per §7)

With the twins merged, the kv9 subtract progressed to a NEW loud wall:
`InvalidBooleanOutput("output loop with fewer than 3 edges…")` on the
bite's cap face at z=0.4 — a TWO-edge loop of one body-circle arc + one
tool-circle arc meeting at the two ruling points (probe: cycle
[(0.2344…, ±0.1872…, 0.4)], exactly the radical-line x). Measurement
verdict: this is a **genuine lens bigon** that the femto-twin artifact
used to subdivide spuriously (the seam vertex is NOT an output junction —
the body-lateral seam is patch-interior; its appearance in the loop was
the twin defect itself). The `from_yang_brep` entrance check predates the
M8 disc∩disc CurveKey bigon support and rejected all <3-edge loops.
Amendment (E-V5): accept a 2-edge loop iff both edges are conic arcs on
DISTINCT curves (`curve_key` inequality — combinatorial, no tolerance);
same-curve pairs and segments remain rejected. The CurveKey manifold
pairing downstream already distinguishes the lens's two arcs.

### 2b. GREEN outcome (2026-07-04, measured)

Both quarantined kv9 tests pass end-to-end (exact-volume oracles) and are
un-quarantined; the KV9-F1 tangency pair stays (separate cycle). Twin
probes show zero sub-floor twins on the fixture. Witnesses green: rewrite
tier, fuzz_boxes (940s) + fuzz_curved (324s) differentials, full
kernel-v2 + yang-rs suites. Full assay: **88 SUPPORTED_CORRECT / 0
WRONG / zero lost vs `baseline-m8foldpair`, +2 gained — F0041 and F0057
ERROR→CORRECT** (corpus parallel-cyl×cyl members blocked on the same
walls). Report banked as `assay_kv2_report.baseline-kv9f3.json`.

## 3. Parameters

None new. No new tolerances (A14.3): the merge criterion stays the
governance feature floor `MIN_FEATURE_SIZE` (A14.2 — two points closer
than the smallest representable feature ARE the same point); the change
is SCAN ELIGIBILITY only.

## 4. Branch table (step-3c merge scan × triangle population)

| # | Path | Trigger | Contract row |
|---|------|---------|--------------|
| E-V1 | Triangle touching a `moved` vertex | UNCHANGED — scanned as today |
| E-V2 | **[fix]** Triangle touching a CONIC-ENDPOINT vertex (any member of the Stage-4 curve-assignment maps: circle / line / ellipse / cone-conic / junction) that is NOT `moved` | Also scanned: a degenerate triangle whose shortest edge < MIN_FEATURE_SIZE merges that edge (same criterion, same watertight-preserving `collapse_vertex`) |
| E-V3 | Triangle touching neither | UNCHANGED — never scanned (planar-only populations remain the step-(2) I6 near-weld's territory) |
| E-V4 | Degenerate scanned triangle whose shortest edge ≥ floor | UNCHANGED — left for `validate_relocated_triangles` / loud stops |
| E-V5 | **[fix, §2a]** `from_yang_brep` loop with exactly 2 edges, both non-Seg conic arcs with UNEQUAL `curve_key`s | Accepted (lens bigon; CurveKey pairing handles it); any other <3-edge non-full loop stays the loud reject |
| E-V6 | **[fix, 2026-07-08 — ERROR-census campaign 4]** `from_yang_brep` loop with exactly 2 edges, exactly ONE `Seg` and one conic arc | Accepted (D-FACE bigon — a circular/elliptic SEGMENT face bounded by a chord and its arc; R0046's face 0: chord 0.197 between two vertices exactly on the arc's r=0.130 circle, ~99° sweep). `classify_edge` has already validated the arc's endpoint membership; the chord's endpoints are the same two vertices by loop closure. Two `Seg`s (zero-area) and same-curve arc pairs remain the loud reject |

## 5. Invariants

- **I1 (single mint):** the kv9 fixture's output B-Rep contains ONE vertex
  per seam junction; no output edge shorter than MIN_FEATURE_SIZE between
  conic-endpoint vertices.
- **I2 (non-regression, byte-identical):** meshes with no unmoved
  sub-floor junction duplicates emit byte-identically (the scan extension
  only ADDS candidate triangles; the merge rule and order are unchanged;
  iteration remains deterministic fixed-point).
- **I3 (E2E acceptance):** the 2 quarantined kv9 tests
  (`parallel_cyl_subtract_exact_volume`, its UNION sibling) pass and are
  un-quarantined in the same PR. Full assay: 0 SUPPORTED_WRONG, zero
  CORRECT lost vs `baseline-m8foldpair`.
- **I4 (watertightness):** every merge goes through `collapse_vertex`
  (half-edge-pairing preserving); the §4.4.3 gate stays after.

## 6. Oracles

- **E2E RED (already red):** the 2 `#[ignore = "KV9-F3 …"]` kv9 tests —
  demonstrated failing un-ignored (G1 gate,
  `TessellationFailed(FaceId(7))`).
- **Probes (diagnostic, kept):** `KV2_OUT_TWIN_PROBE` (output B-Rep twin
  census at the yang→kernel-v2 boundary) and `YANG_S4_TWIN_PROBE`
  (Stage-4-exit twin census with moved/adjacency/assignment context) —
  GREEN shows zero sub-floor twins on the fixture.
- **Witnesses:** kv9_cyl_cyl_special suite (incl. the still-walled
  irreducible-quartic pins), kv6b/kv6c/kv6d curved suites, fuzz_boxes +
  fuzz_curved differential, full assay vs `baseline-m8foldpair`.

## 7. Failure modes / P10 stop criteria

- **GREEN stop:** twins merge but the kv9 tests still fail at a DIFFERENT
  wall → re-measure; do not widen the merge.
- **Fix-shape gate:** any change to the merge CRITERION (floor value, a
  distance tolerance beyond it) → STOP (P9/A14.3). Eligibility only.
- **Assay regression in curved classes** (the scan now reaches previously
  unscanned degenerate slivers on conic edges): a merge that changes a
  currently-CORRECT case's output is only acceptable if the case stays
  CORRECT; any WRONG/lost → revert and re-scope.

## 8. Research basis

- Yang 2025 §4.4.1(b) [#24] (Fig. 11(b): "if an endpoint p of the split
  edge is too close to q, we merge p with q") — the existing N2-1 merge
  this spec extends to its full intended population; §4.4.3 watertightness
  gate unchanged.
- Governance A14.2 (MIN_FEATURE_SIZE floor as the identity criterion) and
  the KV9 I6-weld delegation record (`boolean()` step 2 comment; curved
  inputs' junction reconciliation is Stage-4's responsibility).
- Prior records: `kv2_cdt_triangulation_core` §6a (the G1 unmasking),
  `m8_shared_boundary_identity` §8b (named this target),
  `yang_n2_stage4_cdt_mesh_updating` N2-1 (the merge primitive's cycle).

### 8a. Analytical vs approximate

Not applicable — no SSI change, no approximation; identity hygiene at the
existing exact merge primitive.
