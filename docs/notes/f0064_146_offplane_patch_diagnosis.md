# F0064 (#146) — off-plane vertex in a planar output patch: diagnosis

**Status:** diagnostic increment (no fix shipped). Advances task #146
(Newell-normal class: planar output faces with real-scale off-plane loop
vertices). Sibling cases: R0051, F0058, F0060, C0044, R0009, R0049, R0095
(all `reassembled output would be non-2-manifold` in the same assay cluster).

**Reproduce.** Release single-case + the new producer probe:

```
ASSAY_CASE=F0064 YANG_S6_NONPLANAR_PROBE=1 \
  cargo test -p test-harness --test assay_kv2 --release single_case \
  -- --exact --ignored --nocapture
```

## The wall

`emit_topology` fails its own gross-non-planarity self-check
(`s6-planar-loop-nonplanar`, `stage5_topology.rs`): output patch **face 4**
inherits a `Plane` surface with normal `(0,-1,0)`, `d=-0.162096` (a horizontal
floor at `y=-0.162`), but one of its boundary-cycle vertices sits **0.083 off**
that plane — 400,000× the model band (`TAU_MODEL·(1+|coord|) ≈ 1.97e-7`). The
gate is CORRECT to reject: a genuine planar face cannot carry a vertex that far
off its plane (HARD RULE #4).

The failing cycle (probe output):

```
face=4 input=B inherited=Plane n=(0,-1,0) d=-0.162096  cycles=1
  v=1277 p=(0.098115, -0.078787, 0.973751) dist=-8.33e-2  reloc=None
  v=1280 p=(0.098115, -0.162096, 0.973751) dist=0
  v=1288 p=(0.049174, -0.162096, 0.733860) dist=0
  v=1290 p=(0.066282, -0.162096, 0.733860) dist=0
  v=1292 p=(0.098115, -0.162096, 0.733860) dist=0
```

Four cycle vertices are EXACTLY on the floor; only **v=1277** is off. Note
1277, 1280, 1292 all share **x=0.098115** — they lie on a *vertical wall*
(constant-x plane). 1277 is the wall vertex directly above floor corner 1280
(same x, same z=0.9738; y differs). So a wall triangle reaching up to 1277 has
been grouped into the floor patch. This is the classic #146/#162 symptom
(a wall sliver in a floor patch), but the mechanism below is NOT #162's.

## Hypotheses ELIMINATED this session

1. **Curve relocation moved 1277 off the floor** — REFUTED. `reloc=None`:
   1277 is not in the Stage-4 `relocations` list, and `YANG_V_PROBE=1277`
   shows it carries no curve type (circle/ellipse/cone/line/junction all
   false). It is a genuine, never-relocated Cherchi-arrangement vertex sitting
   on the wall.
2. **Geometric attribution fallback / N4 mis-provenance put a wall triangle on
   the floor face** (the #162 class) — REFUTED at attribution time. A
   structural probe over the *entire* attribution loop (`boolean.rs`) found NO
   triangle attributed to a `Plane` face with any vertex >0.05 off that
   face's plane. #162's max-of-3-vertices `plane_dist` would in any case pick
   the wall (dist 0) over the floor (dist 0.083). So at Stage-6 ENTRY the
   defect does not exist.
3. **`subdivide_loops_at_shared_vertices` (S7) inserted an off-segment
   vertex** — REFUTED. Its insertion gate is `dist² ≤ TAU_WORK²`
   (`stage5_topology.rs:1447`); a 0.083-off vertex can never be inserted onto
   a loop edge. So 1277 is in the RAW `patch_boundary_cycle`, i.e. a real
   floor-patch triangle has it as a corner.
4. **A Stage-4 weld/collapse merged a floor vertex into wall-vertex 1277
   across 0.083** — REFUTED. Instrumenting the shared `collapse_vertex`
   primitive (all of §4.4.1(b) merge, N50 f32-weld, kv15b, §4.5.3) showed the
   largest merge distance is ~9.9e-3 (a legitimate §4.5.3 reversal collapse;
   kv15b is gated at `TAU_MODEL²`). No collapse crosses 0.083, and none
   targets 1277.

## Where the defect must live (next-session start)

By elimination: the wall triangle is **not present at Stage-6 entry** (H2) yet
**is present, floor-attributed, at emission** (the gate). The only stages
between are the Stage-4 mesh mutations, each of which re-runs `compute_phase_a`
(flood-fill + boundary cycles) on the mutated mesh + attribution
(`reconstruct_topology_stage4`, `stage5_topology.rs:160-298`). None of the
*collapses* explains it (H4). The remaining suspects:

- **Cross-boolean chain inheritance.** F0064 is `Extrude 4` — input B is the
  accumulated yang OUTPUT of the prior three ops (lineage-less; that is why the
  geometric fallback runs). If B already carried face 4 as a planar face that a
  *prior* boolean emitted with a marginal vertex, THIS boolean's Stage-1
  re-tessellation of B + the new extrude's arrangement could produce a floor
  patch that reaches the wall. A prior boolean would only have passed its OWN
  `s6-planar-loop-nonplanar` gate if all its vertices were within band — so the
  0.083 offset would have to be introduced by this boolean's re-meshing of B,
  not inherited verbatim.
- **`merge_same_plane_patches`** (`stage4_correct.rs:106`) merges edge-adjacent
  same-plane patches. It should not merge a wall patch into the floor (different
  planes), but its adjacency + plane-equality test is worth auditing against
  this exact pentagon.

The productive next probe is a per-boolean isolation of the chain (the vertex
indices 1277/1280/1292 alias across the ~4 sub-booleans, which muddied the
cross-boolean reads this session). Dump `compute_phase_a`'s patch set + cycles
at Stage-6 ENTRY (`YANG_S6_PATCH_PROBE` + `YANG_S6_CYCLE_DUMP`) for ONLY the
failing sub-boolean and confirm whether face 4's pentagon already reaches 1277
at entry (⇒ inheritance/attribution) or only after a mutation (⇒ mint site).

## Governance note

No fix was attempted (P9/P10): the root cause is not yet localized to a single
layer, and every remaining assay ERROR case is a deep named epic (LRR §4.5.2,
torus SSI, #137 near-tangency, this #146 non-2-manifold class, #145 zigzag).
The R0003/R0026/R0074/R0015 OffCurveBeyondChordBand cluster was also checked
and is NOT a spurious-band class (R0003's Stage-4 cone-ellipse residual is 2×
the band, R0074 is a plane∩torus degree-4 SSI gap) — the N45/N46 band-fix
pattern is exhausted; those are genuinely the Stage-4 LRR / torus epics.
