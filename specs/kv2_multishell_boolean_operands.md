# kv2_multishell_boolean_operands — multi-shell operands re-enter booleans

**Status:** IMPLEMENTED (2026-07-10; amendment 1 same day — see §Amendment 1)
**Crate:** `kernel-v2` (`src/boolean.rs`)
**Milestone:** KV7-F2 (multi-shell re-entry wall), disjoint-lump slice.
**Assay targets:** `UNSUPPORTED(multi-shell)` — C0071, C0072, C0073, C0074,
R0035, R0076.

## Goal

A boolean whose result is spatially disjoint lumps legitimately produces a
multi-shell solid (the auto-union / multi-region disjoint path relies on
this). Today that solid cannot be an *operand* of a later boolean:
`boolean_op` walls ANY operand with more than one shell
(`KernelV2Error::UnsupportedMultiShellBoolean`, PR-KV7). This spec lifts the
wall for the **disjoint-lump** case — the only way multi-shell solids arise
in current chains — while keeping the typed wall for shell clusters that may
contain **voids** (nested shells), whose reassembly semantics remain
unproven.

## Research Basis (P8)

- [#24] Yang et al. 2025 — the pipeline's Stage-2 exact mesh boolean is
  defined on watertight 2-manifold triangle meshes; nothing in §4 requires a
  single connected component per operand.
- Cherchi et al. 2022 (InteractiveAndRobustMeshBooleans) — the in/out
  classification is ray-cast **parity** against each whole input mesh (§5),
  which is component-count-agnostic: inside any lump of A ⇔ inside A.
- Existing production evidence: yang-rs already *emits* multi-component
  outputs (disjoint unions), `from_yang_brep` already splits them into
  shells via `face_components`, and `check_watertight_2manifold`'s Euler
  gate is per-connected-shell (χ = 2−2g per component). The input leg is the
  only missing admission: `to_yang_brep_indexed` already iterates every
  shell of the solid into one (multi-component) `BRep`.

## Parameters

`boolean_op(arena, a, b, op)` — unchanged signature. New behavior depends
only on each operand's shell-cluster structure:

- **lump** := one AABB-overlap cluster of the solid's shells (same
  clustering as `split_solid_into_bodies`: shells whose closed AABBs
  overlap are one cluster).

## Branch table

| # | operand shell clusters (either operand) | behavior |
|---|---|---|
| 1 | single shell (1 cluster of 1) | existing path, byte-identical |
| 2 | ≥2 clusters, every cluster a singleton (disjoint lumps) | ADMIT: convert all shells into one multi-component yang `BRep`, run the pipeline unchanged |
| 3 | any cluster with ≥2 shells (nested AABBs = potential void, or interlocking lumps with overlapping AABBs) | typed `UnsupportedMultiShellBoolean { shells }` (shells = the offending cluster's size) |

Ops (Union/Subtract/Intersect) share the same admission — the mesh boolean
itself is op-agnostic on component count. No new mode branches downstream.

## Invariants

- I1: single-shell × single-shell calls take the identical code path as
  before (no behavior change, bit-for-bit).
- I2: admitted multi-lump operands produce outputs satisfying all existing
  boolean output validation (from_yang_brep walls, `validate_solid`,
  per-shell Euler) — no new relaxations anywhere downstream (P9).
- I3: the void wall is CHECKED, not assumed: an operand with a nested shell
  cluster returns the typed error before any yang conversion.
- I4: input solids stay live and untouched (existing `boolean_op`
  contract) — admission reads shells only.

## Oracles

- Exact volume: union of a 2-lump body (two disjoint boxes) with a bridging
  box; subtract of a box from one lump; intersect selecting one lump.
  Analytic volumes, exact within 1e-9 (all-planar).
- Structure: output body/lump count via `split_solid_into_bodies`; per-case
  watertightness through the standard assay oracles (C0071–C0074 flip
  UNSUPPORTED(multi-shell) → SUPPORTED_CORRECT).
- Lump-consumed subtract: 2-lump body minus a tool engulfing one lump →
  single-lump result with the surviving lump's exact volume.
- Typed wall: hand-assembled nested-shell solid (box shell + interior box
  shell in ONE solid) returns `UnsupportedMultiShellBoolean` for union with
  any tool.

## Failure modes

- Nested/overlapping-AABB shell cluster → `UnsupportedMultiShellBoolean`
  (branch 3, typed, loud). Conservative: interlocking genus-1 lumps with
  overlapping boxes are also walled (under-admission, never silent-wrong).
- Downstream pipeline errors on admitted operands surface exactly as for
  single-shell operands (loud typed walls; measured: R0035 → Stage-4
  `LocalRefinementRequired`, R0076 → `InvalidBooleanOutput` edge-pairing —
  both progress to their honest shared error classes).

## Non-goals

- XOR (multi-shell output reassembly for XOR) — unchanged, gated in yang.

## Amendment 1 (2026-07-10) — measurement kills the void wall too

The branch-3 "keep the wall for AABB-overlapping clusters" plan was based on
the PR-KV7 claim that yang's reassembly cannot rebuild voids. Direct
measurement disproved BOTH of its premises:

1. **The multi-shell corpus cases are not all disjoint lumps.**
   `KV2_MULTISHELL_AABB_PROBE` on the wall site: C0071's operand is a
   genuine VOID (shell AABB [−0.3,0.3]²×[0.35,0.65] fully contained in
   [−1,1]²×[0,1] — a fully-enclosed cut), while R0035's is an
   INTERLOCKING pair (overlapping, non-contained AABBs — the
   `split_solid_into_bodies` under-split shape). AABB-overlap clustering
   walls both.
2. **The pipeline handles voids correctly.** With the wall bypassed
   (`KV2_MULTISHELL_PROBE`, investigation build), C0071–C0074 run
   SUPPORTED_CORRECT end-to-end — exact volume (outer − cavity),
   watertightness, Euler χ all green. This is not luck: the Cherchi 2022
   in/out labeling is ray-cast winding/parity against each whole input
   mesh (§2.4, §5) — a point inside a cavity counts two boundary
   crossings and classifies OUTSIDE, component structure notwithstanding —
   and `from_yang_brep` + `face_components` already assemble
   multi-component outputs into multi-shell solids (production behavior
   since the disjoint-union path). The PR-KV7 "cannot rebuild voids"
   claim described an older reassembly and is STALE.

**Amended behavior: the multi-shell operand wall is REMOVED.** Branch 3 is
gone; every multi-shell operand (disjoint lumps, interlocking lumps,
internal voids) converts through `to_yang_brep_indexed` (which always
emitted every shell) and runs the pipeline unchanged.
`KernelV2Error::UnsupportedMultiShellBoolean` is deleted (no remaining
producer). The two capability-boundary pins flip in the same change (the
milestone un-quarantine pattern): kv6b
`revolve_boolean_output_reentry_stays_typed_wall` becomes a positive
union-with-voided-operand test, and the hand-assembled nested-shell
fixture test is replaced by a production-built void fixture (the
hand-built two-outward-shell solid was INVALID input — overlapping
material claims, not a void — and no production path mints it).

Amended oracles (all exact, all-planar):
- `voided_box` fixture: box [0,4]³ minus box [1,3]³ → 2-shell solid,
  volume 56.
- Union with a side-overlapping box → volume 72; void preserved.
- Subtract a corner box (clear of the cavity) → volume 56 − 0.729.
- Subtract an x-through tunnel that OPENS the cavity (void topology
  destroyed correctly) → volume 56 − 0.72.
- Intersect with a slab straddling the cavity wall → volume 22.
