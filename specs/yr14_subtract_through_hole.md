# PR-YR14 — curved `Subtract`: box − cylinder THROUGH-HOLE (genus 1, χ=0)

**Milestone:** M5 / Phase 2 step (curved `Subtract` topology — the second M5
increment after PR-YR13's blind pocket).
**Predecessor:** PR-YR13 (`box − cylinder` BLIND POCKET, genus 0, cavity-sense
via `BRepFace.reversed`).
**Crate:** `crates/yang-rs/` only.

## Goal (narrow, single cycle)

Extend curved `Subtract` from a **blind pocket** (genus 0, χ=2) to a
**through-hole**: the cylinder passes fully through the box, producing a
cylindrical tunnel. A through-hole box is a single connected, closed, orientable
2-manifold of **genus 1 → χ = 0**.

Everything the through-hole needs already exists:
- The curved `Subtract` cavity-sense (`BRepFace.reversed`, derived from
  `op == Subtract && info.input == InputId::B`) was built in PR-YR13 and is
  reused **unchanged** for the tube wall (`emit_topology`, `src/lib.rs:3420-3486`).
- `op` is already threaded `boolean()` → `reconstruct_topology_stage4` →
  `emit_topology` (`src/lib.rs:2561, 3331, 3392`).
- Annular planar faces with circular holes (box top + bottom each lose a disk)
  are handled by PR-YR5c's multi-cycle / `positive_count == 1` machinery
  (`src/lib.rs:3537-3560`).
- The exact `Circle` rim edges (cylinder ∩ box plane) come from the op-agnostic
  P3/Stage-4 SSI path already wired (PR-YR9/YR10).

What is genuinely new vs YR13:
- Genus-1 topology (χ=0), single shell.
- **Two** rim `Circle` edges: cylinder ∩ box-top AND cylinder ∩ box-bottom.
- The tube wall spans the full box thickness; **no pocket floor** (the cylinder's
  caps lie outside the box). The wall patch has **two** boundary cycles (top rim +
  bottom rim, each N edges) — the curved branch already emits outer + inner loops.

## The one production change: generalize the per-shell Euler gate

The blocker is `check_watertight_2manifold` (`src/lib.rs:1705`), whose per-shell
Euler gate asserts **`V − E + F == 2`** ("each shell is a sphere") and returns
`NonManifoldOutput` otherwise (`src/lib.rs:1776`). A genus-1 shell has χ = 0, so
the through-hole result is **wrongly rejected today**.

Replace the strict equality with a parity/upper-bound gate:

```rust
let chi = v - e + f;
// A closed orientable 2-manifold shell has χ = 2 − 2g for integer genus g ≥ 0,
// so χ is EVEN and ≤ 2. Accept any such χ (sphere χ=2 / g=0; through-hole
// χ=0 / g=1; …). Reject odd χ or χ > 2 — impossible for a closed orientable
// manifold → a real defect (NOT a tolerance/fallback relaxation).
if chi > 2 || chi.rem_euclid(2) != 0 {
    return Err(YangError::NonManifoldOutput);
}
```

`rem_euclid(2)` reads unambiguously for negative even χ (g≥2 → χ ≤ −2).

**KEEP strict, do NOT touch:**
- The directed half-edge pairing loop (`src/lib.rs:1714-1719`) — genus-independent,
  catches true non-manifold. Stays exactly as-is.
- This is the **only** relaxation. No tolerance widening, no fallback path
  (P9/P10): an unpaired half-edge OR odd/`>2` χ still returns
  `NonManifoldOutput`.

Also update the now-stale prose: the doc comment "Euler characteristic … must
be 2" (`src/lib.rs:1701-1704`) and the inline "each of which must be a sphere
(χ = 2)" (`src/lib.rs:1721-1723`) → state χ = 2 − 2g, g ≥ 0.

No other production code changes. `emit_topology`'s curved branch already sets
`reversed` correctly for the tube wall, and the planar branch already builds the
annular box faces.

## Branch table (`emit_topology`, `src/lib.rs`) — UNCHANGED from YR13

| Branch | Surface | Sense encoding | `reversed` |
|---|---|---|---|
| Planar (`src/lib.rs:3489+`) | `Plane` | possibly-flipped `Plane.normal` (winding-derived) | `false` (always) |
| Curved (`src/lib.rs:3420+`) | `Cylinder` | surface inherited UNCHANGED; flag records flip | `op == Subtract && info.input == InputId::B` |

The tube wall is a single curved patch with **two** boundary cycles (top rim +
bottom rim). The curved branch's outer-loop selection (most edges; tie-break
lowest min start-vertex) and inner-loop emission already cover this — both cycles
have N edges, so the tie-break decides outer vs inner; either assignment is a
valid B-Rep face with one outer + one inner loop.

## Invariants

Reuse PR-YR13's **I-rev1..I-rev4** verbatim (consistency, no double-flip, exact
params, byte-identity of Union/planar). Add:

- **I-genus:** the per-shell Euler gate accepts a closed orientable 2-manifold
  shell of any genus — χ = 2 − 2g for g ≥ 0 (χ even, ≤ 2) — and rejects odd χ or
  χ > 2 (impossible for a closed orientable manifold → a real defect). The
  directed half-edge pairing check is unchanged and genus-independent.

## Oracles (RED — `crates/yang-rs/tests/yr14_through_hole.rs`)

Drive the public `boolean(&box, &cyl, BoolOp::Subtract, &mock)` with a hand-built
`LabeledArrangement` (`LabelMock`), modeled on `tests/yr13_subtract_cylinder.rs`:
box `[-2,-2,0]..[2,2,2]`; cylinder axis +Z, r=1, spanning **z=−0.5..2.5** (fully
penetrating, caps OUTSIDE the box). Mock geometry:
- Box bottom annulus (z=0, hole = bottom rim ring r=1) + box top annulus (z=2,
  hole = top rim ring) + 4 box sides — all label 0, `inside=[false,false]`,
  authored outward (global reversal like YR13's `push_box`).
- Tube wall: rim ring (z=2) ↔ floor rim ring (z=0), label 1, `inside=[true,false]`,
  authored so post-`flip_for_op` winding points toward-axis (reuse YR13's
  `push_cyl` cancellation trick). **No floor cap.**
- Verify by scratch math the mock is watertight + χ=0 BEFORE asserting (the mock
  itself must be a valid genus-1 closed shell, else the test is meaningless).

1. **Through-hole succeeds** + watertight (`unpaired_half_edges == 0`) + **χ == 0**
   asserted explicitly as genus 1; positive signed volume (outward).
2. **Gate not weakened** (RED contract; Adversary re-verifies): three
   defect-injected meshes each still return `Err(NonManifoldOutput)` — (a) a mesh
   with an unpaired half-edge, (b) odd χ (χ=1), (c) χ > 2 (χ=4). Drive via the
   public `boolean()` with a mock whose arrangement mesh carries the defect,
   asserting the loud reject.
3. **Cavity-sense**: the tube wall is `Surface::Cylinder` (exact input params),
   `reversed == true`; sampled effective normal (−radial) points toward axis;
   witness actual mesh-triangle winding toward-axis (PART B style from YR13 O1).
4. **Two exact `Circle` rim edges**: the output has ≥2 `Curve::Circle` edges; each
   lies on both the cylinder lateral and its box-face plane to `TAU_MODEL` (one at
   z=2 top, one at z=0 bottom).
5. **Sidecar `Subtract` mesh-parity** (env-gated via `SidecarBoolean::from_env()`,
   LOUD eprintln skip) + **determinism** (two runs byte-identical verts/tris/faces
   incl. `reversed`). The direct mock path is the GREEN gate; the sidecar is an
   independent check.

## Failure modes

- **F-genus1 — gate over-relaxed:** weakening the gate beyond "χ=2−2g, reject
  odd/`>2`" (e.g. dropping the half-edge pairing check, or accepting odd χ).
  Oracle 2 catches it.
- **F-genus2 — two-rim wall does not close:** the curved branch fails to extract
  both wall cycles, or half-edge pairing fails on the tube. Oracle 1 catches it →
  but this is a **STOP** (see below), not an improvised fix.
- Reuse PR-YR13's F-rev1..F-rev3 (wrong sense, double-flip, param perturbation),
  caught by oracles 3.

## STOP conditions (P9/P10)

- Genus-1 output cannot be made valid without weakening the manifold/χ gate
  **beyond** "χ=2−2g, reject odd/`>2`".
- The two-rim tube-wall reassembly cannot close honestly (e.g. boundary-cycle
  extraction or half-edge pairing fails — `patch_boundary_cycle` does not extract
  both wall cycles).

Either → halt the cycle and report what was learned; do not improvise.

## Research basis

- **Yang et al. 2025 §4.4.2 / §4.5** — Stage-6 B-Rep reassembly: a kept face
  inherits its analytical surface; a subtracted subtrahend's bounding faces are
  cavity walls whose outward orientation reverses. A through-hole is the same
  reassembly with the tube wall spanning the full solid (two rims, no cap) and the
  result a genus-1 shell. (`refs/text/yang2025_hybrid_boolean.txt`.)
- **Euler–Poincaré** — a closed orientable 2-manifold of genus g has
  χ = V − E + F = 2 − 2g. A through-hole has g = 1 → χ = 0.

## On completion

- `docs/yang_functional_roadmap.md`: add **PR-YR14 — through-hole genus-1
  Subtract; per-shell Euler gate generalized to χ=2−2g ✅ DONE** in the YR13
  style. Remaining: sphere/cone cavities, the side-face/corner (triple-point)
  guard, box-as-subtrahend.
- Refresh the deferral prose in `src/lib.rs:106-112` (drop "through-hole (genus 1,
  χ=0)" from the still-deferred list).
