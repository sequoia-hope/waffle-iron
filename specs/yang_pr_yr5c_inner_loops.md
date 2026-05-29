# Spec: PR-YR5c — B-Rep faces with inner loops (holes)

**Status:** active (roadmap: lifts the M3 `NonManifoldOutput` bucket)
**Feature cycle:** yang-yr5c
**Roles (P5):** Spec Writer = Manager; Test Author and Implementer are distinct agents.

## Goal

When one solid pierces a hole through another's face, the result face is an
**annulus** (outer boundary + ≥1 inner loop). `reconstruct_topology` currently
rejects any patch with more than one boundary cycle
(`patch_boundary_cycle`, lib.rs:1090 → `NonManifoldOutput`) — the sole failure
bucket in the M3 fuzz (~25% of cases). This cycle adds inner-loop support so
those booleans succeed with a correct holed-face B-Rep.

**Fix is B-Rep-topology-only.** The output mesh (kept sub-mesh of sidecar
triangles) already tessellates holed faces correctly (mesh volume/watertight/
Euler oracles already pass); reconstruct merely errored. No mesh change.

## Scope

**In:** faces with one or more inner loops from interior-pierce interpenetration
of convex planar solids (Union/Intersect/Subtract). **Out:** nested holes
(hole-in-hole — doesn't arise convex-convex; error if seen); genuinely
non-manifold patches (T-junction / dead-end — keep erroring as `NonManifoldOutput`);
coplanar (M8); XOR (gated); curved/SSI (M5).

## Data flow change

`reconstruct_topology`, per patch:
1. `patch_boundary_cycle` now returns **all** boundary cycles (was: one, else error).
2. Classify each cycle outer vs inner; build `BRepFace { surface, outer_loop, inner_loops }`.

## Branch table

| # | Patch boundary | Action |
|---|---|---|
| L0 | one cycle | `outer_loop` = that cycle; `inner_loops = []` (unchanged behavior) |
| L1 | N cycles, exactly one with positive signed area | positive = `outer_loop`; the (N−1) negative = `inner_loops` (holes) |
| E1 | dead-end / T-junction during a cycle walk | `Err(NonManifoldOutput)` (genuine non-manifold — unchanged) |
| E2 | a cycle's |signed area| < `MIN_FEATURE_SIZE²` (degenerate loop) | `Err(NonManifoldOutput)` |
| E3 | not exactly one positive-area cycle (0 or ≥2) | `Err(NonManifoldOutput)` (disconnected / nested — out of scope) |

**Classification (L1):** for each cycle, Newell area-vector `N_loop = Σ (v_i × v_{i+1})`
over its ordered loop vertices; signed area along the face normal = `N_loop · n̂_face`.
The kept tris are outward-oriented (M3 `flip_for_op` + arrangement), so the outer
boundary is CCW-from-outside (`> 0`) and holes are CW-from-outside (`< 0`).

## `BRepFace`

```
pub struct BRepFace {
    pub surface: Surface,
    pub outer_loop: Vec<u32>,          // CCW from outside (edge indices)
    pub inner_loops: Vec<Vec<u32>>,    // each CW from outside (a hole); empty for simple faces
}
```
~34 existing struct-literal sites get `inner_loops: Vec::new()` (mechanical;
Test Author migrates the test fixtures, Implementer the 1 production site).

## Invariants

- **I1 (loop closure):** every outer/inner loop is a closed directed cycle
  (`edges[loop[i]].end == edges[loop[(i+1)%n]].start`).
- **I2 (orientation):** `outer_loop` signed area (Newell·n̂) > 0; each inner loop < 0.
- **I3 (B-Rep manifold):** over ALL face loops (outer + inner) as directed edges,
  every directed edge `(a,b)` has exactly one reverse `(b,a)` — holed faces stitch
  closed with the tunnel-wall faces. (The key new structural oracle.)
- **I4 (surface tier, A15.5):** each output face keeps its source input face's `Surface`.
- **I5 (mesh unchanged):** output-mesh volume/watertight/Euler match the sidecar
  reference (the fix doesn't touch the mesh; it stops the reconstruct error).

## Oracles (P1, DoD §1)

Canonical: cube `A=[0,1]³` minus rod `B=[0.3,0.7]×[0.3,0.7]×[−0.5,1.5]` (interior
pierce; non-coplanar — B planes {0.3,0.7,−0.5,1.5} disjoint from A's {0,1}).
Analytic: overlap `A∩B = [0.3,0.7]²×[0,1] = 0.16`; **subtract 0.84, intersect
0.16, union 1.16**. Subtract produces a square tunnel → **z=0 and z=1 faces are
annuli** (each `inner_loops.len() == 1`).
- `boolean(A,B,Subtract)` → **Ok** (was `NonManifoldOutput`) — the core fix.
- Exactly **2** output faces have `inner_loops.len() == 1`.
- Loop orientation I2; B-Rep manifold I3.
- Output-mesh signed volume == 0.84 (±TAU_MODEL); watertight; differential Euler.
- **Edge case:** rod through part-way (e.g. z=[−0.5,0.5]) → 1 holed face + 1 plain
  (the far face untouched). **Regression:** an M3 corner-clip case still yields
  simple faces (`inner_loops` empty) — no spurious holes.

## Failure modes

- T-junction / dead-end → `NonManifoldOutput` (E1, unchanged).
- Degenerate / non-classifiable loops → `NonManifoldOutput` (E2/E3).
- Nested holes → `NonManifoldOutput` (out of scope).

## Research basis

- **Yang 2025** §4.4.2 — Stage 6 B-Rep reassembly produces faces with loops.
- Standard B-Rep face model (outer + inner loops; e.g. Mäntylä [#16], Stroud [#33]).
- Loop orientation via Newell area-vector · normal (the M1 Stage-1 technique).
- **A15.5** surface tier preservation; **A14.3** shared tolerance (`MIN_FEATURE_SIZE`).

## Definition of Done (DoD §1)

Spec (this file); RED→GREEN separate commits; every branch (L0/L1/E1/E2/E3)
tested; numeric (volume) + structural (holed-face count, loop orientation, B-Rep
manifold) oracles; canonical (cube-rod) + edge (part-way) + regression
(corner-clip, no spurious holes) cases; no test weakened; CI gate (fmt + clippy
-D warnings) clean; the `#[ignore]` fuzz shows the NonManifoldOutput bucket
shrink with SILENT_WRONG still 0.
