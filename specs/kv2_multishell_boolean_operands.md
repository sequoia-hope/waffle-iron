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

## Amendment 2 (2026-10-12) — the splitter's box is the shell's CONSERVATIVE box, not its vertex hull

**Finding (P0041, prospector seed 4 index 42).** `split_solid_into_bodies`
clustered shells by the axis-aligned hull of each shell's loop VERTICES. For a
curved shell that hull under-covers the surface it bounds — a plain cylinder's
B-Rep carries exactly two vertices, both on its seam, so its "box" was a line
segment on the lateral. A square cavity sunk into a cylinder boss (an engine
cut whose direction-less sweep ran INTO the body from a plane 10 below the top
cap; the subtract took the clear-operands set-algebra path and minted the
2-shell solid correctly) therefore clustered with nothing, the splitter
returned the cavity as a second body, the adapter handed the engine two
bodies, and the next cut — a wider prism sharing the cavity's floor and
ceiling planes — ran as two subtracts: the boss alone (fine) and the
inside-out cavity shell alone, whose output kept the cavity's floor and
ceiling squares with every edge used once:
`InvalidBooleanOutput("an undirected output edge is not used by exactly two
directed edges")`. The same proxy defect the seed-4 WRONG rows found in the
engine's cut auto-reversal (`specs/assay_prospector.md` §8 lessons 2 and 4):
B-Rep vertices do not represent a curved body's bulk.

**Rule.** The splitter clusters by `introspect::conservative_shell_aabb` —
the per-shell form of the solid bound `conservative_aabb` already computes
(vertex hull grown by every conic edge's circle box, a cylinder or cone face by
its slab, a sphere or torus by its centre box), so a closed shell's box covers
every point of the surface it bounds. A shell with no cheap bound (an edge on
an unbounded-bulge curve, `None`) overlaps EVERY other shell — the splitter's
standing rule is never to over-split, and under-splitting is a display
artifact. Nothing else changes: closed-interval overlap, union-find,
deterministic order.

**Oracles.** kernel-v2 `tests/void_shell_stays_with_its_lump.rs`: the boss +
cavity solid splits to ONE body (RED before: 2); the P0041 chain completes,
one shell, exact volume π·110²·210 − 100·33078.129 (the diamond ∩ disc area by
a 6000-column integration) within 5e-4. The disjoint-lump tests of this spec
are unchanged (boxes of planar solids ARE their vertex hulls). Corpus: P0041
ERROR → SUPPORTED_CORRECT 0.9 s; its un-minimized 8-op lineage completes.


## Amendment 3 (2026-10-13) — a body is an OUTWARD shell; a void files with the lump whose box contains it

**Finding (CI on the Amendment 2 push, `render_view_parity`).** With honest
boxes, box-OVERLAP union-find glued two disjoint lumps into one body: R0052 is
a gear revolve beside an oblique gear extrude that never meet, and their
conservative boxes overlap although their vertex hulls had not. The render-view
parity golden pinned R0052 at two bodies and went red at one. Amendments 2 and
3 are the same defect seen from both sides: box overlap only approximates
containment in one direction, and the vertex hull approximated the box in the
other. The corpus did not move (the assay checks volume, not body count); the
body count is a render-view and engine-body fact.

**Rule.** `split_solid_into_bodies` clusters by ORIENTATION and CONTAINMENT
(`shell_body_clusters`):

1. A shell whose signed volume is positive (`mass::shell_signed_volume` — the
   Q3 face integrators, exact where a face has an exact arm, render triangles
   otherwise; only the sign is read) is a LUMP and a body of its own.
2. A shell whose signed volume is not positive is a VOID. A valid solid
   encloses every void in some lump, so the void's extent lies inside that
   lump's and its box inside the lump's conservative box
   (`introspect::conservative_shell_aabb`). The void files with the innermost
   such lump — the smallest containing box — which places a matryoshka
   (lump ⊃ void ⊃ lump) correctly.
3. Never over-split: a void no lump's box contains (its own box
   over-estimated by a conic edge's full circle) files with the lump whose
   box overlaps it most, then with the first lump; a shell with no cheap bound
   (`None`) is treated as contained by and containing everything; a solid with
   no outward shell is one body.

Bodies come out in the order their lumps appear in the solid's shell list.

**Oracles.** kernel-v2 `tests/disjoint_body_split.rs`:
`disjoint_lumps_with_overlapping_boxes_are_two_bodies` (a cylinder and a block
tucked into its box corner outside the disc → 2 bodies; RED under Amendment 2's
box overlap) and `a_void_files_with_its_enclosing_lump_not_an_overlapping_neighbour`
(the same with a cavity whose box reaches the block's → the cavity stays with
the cylinder, `[2, 1]` shells, exact voided volume). The Amendment 2 pins
(`void_shell_stays_with_its_lump.rs`) and the Amendment 1 operand oracles are
unchanged and green. `wasm-bridge/tests/render_view_parity.rs` green again
with the unchanged golden (R0052 back to two bodies).
